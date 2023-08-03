use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::io;
use std::pin::Pin;
use std::str::FromStr;
use std::sync::mpsc;
use std::task::Poll;
use std::time::Duration;

pub use laplace_wasm::route::gossipsub::Message;
use libp2p::futures::{executor, Future, StreamExt};
use libp2p::gossipsub::{self, IdentTopic as Topic, MessageAuthenticity, MessageId, ValidationMode};
use libp2p::identity::Keypair;
use libp2p::multiaddr::Protocol;
use libp2p::swarm::SwarmEvent;
use libp2p::{mdns, Multiaddr, PeerId, Swarm};
use thiserror::Error;

use crate::service;

pub type Sender = mpsc::Sender<Message>;
pub type Receiver = mpsc::Receiver<Message>;

pub struct GossipsubService {
    swarm: Swarm<gossipsub::Behaviour>,
    swarm_discovery: Swarm<mdns::async_io::Behaviour>,
    dial_ports: Vec<u16>,
    topic: Topic,
    receiver: Receiver,
    lapp_service_sender: service::lapp::Sender,
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
        lapp_service_sender: service::lapp::Sender,
    ) -> Result<(Self, Sender), Error> {
        let transport = executor::block_on(libp2p::development_transport(keypair.clone()))?;
        let message_id_fn = |message: &gossipsub::Message| {
            let mut hasher = DefaultHasher::new();
            message.data.hash(&mut hasher);
            MessageId::from(hasher.finish().to_string())
        };
        let gossipsub_config = gossipsub::ConfigBuilder::default()
            .heartbeat_interval(Self::HEARTBEAT_INTERVAL)
            .validation_mode(ValidationMode::Strict)
            .message_id_fn(message_id_fn)
            .build()
            .map_err(|err| Error::GossipsubUninit(err.into()))?;
        let mut gossipsub_behaviour =
            gossipsub::Behaviour::new(MessageAuthenticity::Signed(keypair.clone()), gossipsub_config)
                .map_err(|err| Error::GossipsubUninit(err.into()))?;

        let topic = Topic::new(topic_name);
        gossipsub_behaviour
            .subscribe(&topic)
            .map_err(Error::GossipsubSubscribtionError)?;
        for peer_id in explicit_peers {
            gossipsub_behaviour.add_explicit_peer(peer_id);
        }

        let mut swarm = Swarm::with_threadpool_executor(transport, gossipsub_behaviour, peer_id);
        swarm.listen_on(address)?;

        let transport = executor::block_on(libp2p::development_transport(keypair))?;
        let behaviour = mdns::async_io::Behaviour::new(mdns::Config::default(), peer_id)?;
        let mut swarm_discovery = Swarm::with_threadpool_executor(transport, behaviour, peer_id);
        swarm_discovery.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

        let (sender, receiver) = mpsc::channel();
        Ok((
            Self {
                swarm,
                swarm_discovery,
                dial_ports,
                topic,
                receiver,
                lapp_service_sender,
                peers: Default::default(),
            },
            sender,
        ))
    }
}

impl Future for GossipsubService {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        while let Poll::Ready(Some(event)) = self.swarm_discovery.poll_next_unpin(cx) {
            match event {
                SwarmEvent::Behaviour(mdns::Event::Discovered(peers)) => {
                    for (peer_id, address) in peers {
                        log::info!("MDNS discovered {peer_id} {address}");
                        let addresses = self.peers.entry(peer_id).or_default();
                        if !addresses.contains(&address) {
                            addresses.push(address);
                        }
                    }
                },
                SwarmEvent::Behaviour(mdns::Event::Expired(expired)) => {
                    for (peer_id, address) in expired {
                        log::info!("MDNS expired {peer_id} {address}");
                        self.peers.remove(&peer_id);
                    }
                },
                SwarmEvent::NewListenAddr { address, .. } => log::info!("MDNS listening on {address:?}"),
                SwarmEvent::IncomingConnection {
                    local_addr,
                    send_back_addr,
                } => log::info!("MDNS incoming connection {local_addr}, {send_back_addr}"),
                _ => break,
            }
        }

        loop {
            if let Err(err) = match self.receiver.try_recv() {
                Ok(Message::Text { msg, .. }) => {
                    let topic = self.topic.clone();
                    log::info!("Publish message: {msg}");
                    self.swarm
                        .behaviour_mut()
                        .publish(topic, msg)
                        .map(drop)
                        .map_err(Error::GossipsubPublishError)
                },
                Ok(Message::Dial(peer_id)) => {
                    log::info!("Dial peer: {peer_id}");
                    PeerId::from_str(&peer_id)
                        .map_err(|err| Error::ParsePeerIdError(format!("{err:?}")))
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
                                    log::info!("Dial address: {address}");
                                    self.swarm.dial(address.clone()).map_err(Error::DialError)?;
                                }
                                Ok(())
                            } else {
                                Err(Error::DialError(libp2p::swarm::DialError::NoAddresses))
                            }
                        })
                },
                Ok(Message::AddAddress(address)) => {
                    log::info!("Add address: {address}");
                    Multiaddr::from_str(&address)
                        .map_err(Error::WrongMultiaddr)
                        .and_then(|address| self.swarm.dial(address).map_err(Error::DialError))
                },
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => return Poll::Ready(()),
            } {
                log::error!("P2P error for topic \"{}\": {err:?}", self.topic);
            }
        }

        while let Poll::Ready(Some(event)) = self.swarm.poll_next_unpin(cx) {
            match event {
                SwarmEvent::Behaviour(gossipsub::Event::Message {
                    propagation_source: peer_id,
                    message_id,
                    message,
                }) => {
                    let text = String::from_utf8_lossy(&message.data); // todo: catch error
                    log::info!("Got message: {text} with id: {message_id} from peer: {peer_id:?}");
                    if message.topic == self.topic.hash() {
                        // todo: use async send
                        if let Err(err) =
                            self.lapp_service_sender
                                .send(service::lapp::Message::GossipSub(Message::Text {
                                    peer_id: peer_id.to_base58(),
                                    msg: text.to_string(),
                                }))
                        {
                            log::error!("Error occurs when send to lapp service: {err:?}");
                        }
                    }
                },
                SwarmEvent::NewListenAddr { address, .. } => log::info!("Listening on {address:?}"),
                SwarmEvent::IncomingConnection {
                    local_addr,
                    send_back_addr,
                } => log::info!("Incoming connection {local_addr}, {send_back_addr}"),
                _ => break,
            }
        }
        Poll::Pending
    }
}

pub fn decode_keypair(bytes: &mut [u8]) -> Result<Keypair, Error> {
    Ok(Keypair::from_protobuf_encoding(bytes)?)
}

pub fn decode_peer_id(bytes: &[u8]) -> Result<PeerId, Error> {
    Ok(PeerId::from_bytes(bytes)?)
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("Hash error: {0}")]
    HashError(#[from] libp2p::multihash::Error),

    #[error("Fail identity decode: {0}")]
    IdentityDecodeError(#[from] libp2p::identity::DecodingError),

    #[error("Wrong multiaddr: {0}")]
    WrongMultiaddr(#[from] libp2p::multiaddr::Error),

    #[error("Dial error: {0}")]
    DialError(#[from] libp2p::swarm::DialError),

    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("Gossipsub uninitialize: {0}")]
    GossipsubUninit(String),

    #[error("Gossipsub subscription error: {0:?}")]
    GossipsubSubscribtionError(libp2p::gossipsub::SubscriptionError),

    #[error("Gossipsub publish error: {0:?}")]
    GossipsubPublishError(libp2p::gossipsub::PublishError),

    #[error("Parse peer ID error: {0}")]
    ParsePeerIdError(String),

    #[error("Transport error: {0}")]
    TransportError(#[from] libp2p::TransportError<io::Error>),
}
