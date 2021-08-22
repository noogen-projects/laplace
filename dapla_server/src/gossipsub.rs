use std::{
    collections::{hash_map::DefaultHasher, HashMap},
    hash::{Hash, Hasher},
    pin::Pin,
    str::FromStr,
    sync::mpsc,
    task::Poll,
    time::Duration,
};

use libp2p::{
    futures::{executor, Future, StreamExt},
    gossipsub::{
        Gossipsub, GossipsubConfigBuilder, GossipsubEvent, GossipsubMessage, IdentTopic as Topic, MessageAuthenticity,
        MessageId, ValidationMode,
    },
    identity::{ed25519, Keypair},
    mdns::{Mdns, MdnsConfig, MdnsEvent},
    multiaddr::Protocol,
    swarm::SwarmEvent,
    Multiaddr, PeerId, Swarm,
};
use log::{error, info};

use crate::daps::service;

pub use {self::error::Error, dapla_wasm::route::gossipsub::Message};

pub mod error;

pub type Sender = mpsc::Sender<Message>;
pub type Receiver = mpsc::Receiver<Message>;

pub struct GossipsubService {
    swarm: Swarm<Gossipsub>,
    swarm_discovery: Swarm<Mdns>,
    dial_ports: Vec<u16>,
    topic: Topic,
    receiver: Receiver,
    dap_service_sender: service::Sender,
    peers: HashMap<PeerId, Vec<Multiaddr>>,
}

impl GossipsubService {
    /// How often heartbeat pings are sent
    const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

    pub fn new(
        keypair: Keypair,
        peer_id: PeerId,
        explicit_peers: &[PeerId],
        address: Multiaddr,
        dial_ports: Vec<u16>,
        topic_name: impl Into<String>,
        dap_service_sender: service::Sender,
    ) -> Result<(Self, Sender), Error> {
        let transport = executor::block_on(libp2p::development_transport(keypair.clone()))?;
        let message_id_fn = |message: &GossipsubMessage| {
            let mut hasher = DefaultHasher::new();
            message.data.hash(&mut hasher);
            MessageId::from(hasher.finish().to_string())
        };
        let gossipsub_config = GossipsubConfigBuilder::default()
            .heartbeat_interval(Self::HEARTBEAT_INTERVAL)
            .validation_mode(ValidationMode::Strict)
            .message_id_fn(message_id_fn)
            .build()
            .map_err(|err| Error::GossipsubUninit(err.into()))?;
        let mut gossipsub_behaviour = Gossipsub::new(MessageAuthenticity::Signed(keypair.clone()), gossipsub_config)
            .map_err(|err| Error::GossipsubUninit(err.into()))?;

        let topic = Topic::new(topic_name);
        gossipsub_behaviour
            .subscribe(&topic)
            .map_err(Error::GossipsubSubscribtionError)?;
        for peer_id in explicit_peers {
            gossipsub_behaviour.add_explicit_peer(peer_id);
        }

        let mut swarm = Swarm::new(transport, gossipsub_behaviour, peer_id);
        swarm.listen_on(address)?;

        let transport = executor::block_on(libp2p::development_transport(keypair))?;
        let behaviour = executor::block_on(Mdns::new(MdnsConfig::default()))?;
        let mut swarm_discovery = Swarm::new(transport, behaviour, peer_id);
        swarm_discovery.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

        let (sender, receiver) = mpsc::channel();
        Ok((
            Self {
                swarm,
                swarm_discovery,
                dial_ports,
                topic,
                receiver,
                dap_service_sender,
                peers: Default::default(),
            },
            sender,
        ))
    }
}

impl Future for GossipsubService {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        loop {
            match self.swarm_discovery.poll_next_unpin(cx) {
                Poll::Ready(Some(event)) => match event {
                    SwarmEvent::Behaviour(MdnsEvent::Discovered(peers)) => {
                        for (peer_id, address) in peers {
                            info!("MDNS discovered {} {}", peer_id, address);
                            let addresses = self.peers.entry(peer_id).or_default();
                            if !addresses.contains(&address) {
                                addresses.push(address);
                            }
                        }
                    }
                    SwarmEvent::Behaviour(MdnsEvent::Expired(expired)) => {
                        for (peer_id, address) in expired {
                            info!("MDNS expired {} {}", peer_id, address);
                            self.peers.remove(&peer_id);
                        }
                    }
                    SwarmEvent::NewListenAddr { address, .. } => info!("MDNS listening on {:?}", address),
                    SwarmEvent::IncomingConnection {
                        local_addr,
                        send_back_addr,
                    } => info!("MDNS incoming connection {}, {}", local_addr, send_back_addr),
                    _ => break,
                },
                Poll::Ready(None) | Poll::Pending => break,
            }
        }

        loop {
            if let Err(err) = match self.receiver.try_recv() {
                Ok(Message::Text { msg, .. }) => {
                    let topic = self.topic.clone();
                    info!("Publish message: {}", msg);
                    self.swarm
                        .behaviour_mut()
                        .publish(topic, msg)
                        .map(drop)
                        .map_err(Error::GossipsubPublishError)
                }
                Ok(Message::Dial(peer_id)) => {
                    info!("Dial peer: {}", peer_id);
                    PeerId::from_str(&peer_id)
                        .map_err(|err| Error::ParsePeerIdError(format!("{:?}", err)))
                        .and_then(|peer_id| {
                            if let Some(mut address) = self
                                .peers
                                .get(&peer_id)
                                .and_then(|addresses| addresses.first())
                                .cloned()
                            {
                                for port in self.dial_ports.clone() {
                                    address.pop();
                                    address.push(Protocol::Tcp(port));
                                    info!("Dial address: {}", address);
                                    self.swarm.dial_addr(address.clone()).map_err(Error::DialError)?;
                                }
                                Ok(())
                            } else {
                                Err(Error::DialError(libp2p::swarm::DialError::NoAddresses))
                            }
                        })
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => return Poll::Ready(()),
            } {
                error!("P2P error for topic \"{}\": {:?}", self.topic, err);
            }
        }

        loop {
            match self.swarm.poll_next_unpin(cx) {
                Poll::Ready(Some(event)) => match event {
                    SwarmEvent::Behaviour(GossipsubEvent::Message {
                        propagation_source: peer_id,
                        message_id,
                        message,
                    }) => {
                        let text = String::from_utf8_lossy(&message.data); // todo: catch error
                        info!("Got message: {} with id: {} from peer: {:?}", text, message_id, peer_id);
                        if message.topic == self.topic.hash() {
                            // todo: use async send
                            if let Err(err) =
                                self.dap_service_sender
                                    .try_send(service::Message::GossipSub(Message::Text {
                                        peer_id: peer_id.to_base58(),
                                        msg: text.to_string(),
                                    }))
                            {
                                log::error!("Error occurs when send to dap service: {:?}", err);
                            }
                        }
                    }
                    SwarmEvent::NewListenAddr { address, .. } => info!("Listening on {:?}", address),
                    SwarmEvent::IncomingConnection {
                        local_addr,
                        send_back_addr,
                    } => info!("Incoming connection {}, {}", local_addr, send_back_addr),
                    _ => break,
                },
                Poll::Ready(None) | Poll::Pending => break,
            }
        }
        Poll::Pending
    }
}

pub fn decode_keypair(bytes: &mut [u8]) -> Result<Keypair, Error> {
    Ok(Keypair::Ed25519(ed25519::Keypair::decode(bytes)?))
}

pub fn decode_peer_id(bytes: &[u8]) -> Result<PeerId, Error> {
    Ok(PeerId::from_bytes(bytes)?)
}
