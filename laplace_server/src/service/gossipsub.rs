use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::io;
use std::str::FromStr;
use std::time::Duration;

pub use laplace_wasm::route::gossipsub::Message;
use libp2p::futures::StreamExt;
use libp2p::gossipsub::{self, IdentTopic as Topic, MessageAuthenticity, MessageId, ValidationMode};
use libp2p::identity::Keypair;
use libp2p::multiaddr::Protocol;
use libp2p::swarm::{NetworkBehaviour, SwarmBuilder, SwarmEvent};
use libp2p::{mdns, Multiaddr, PeerId, Swarm};
use thiserror::Error;
use truba::{Context, Sender, UnboundedMpscChannel};

use crate::service::lapp::LappServiceMessage;
use crate::service::Addr;

#[derive(Debug)]
pub struct GossipsubServiceMessage(pub Message);

impl truba::Message for GossipsubServiceMessage {
    type Channel = UnboundedMpscChannel<Self>;
}

#[derive(NetworkBehaviour)]
struct GossipsubServiceBehaviour {
    gossipsub: gossipsub::Behaviour,
    mdns: mdns::tokio::Behaviour,
}

pub struct GossipsubService {
    swarm: Swarm<GossipsubServiceBehaviour>,
    dial_ports: Vec<u16>,
    topic: Topic,
    lapp_service_sender: Sender<LappServiceMessage>,
    peers: HashMap<PeerId, Vec<Multiaddr>>,
}

impl GossipsubService {
    /// How often heartbeat pings are sent
    const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

    #[allow(clippy::too_many_arguments)]
    pub fn run(
        ctx: Context<Addr>,
        actor_id: Addr,
        keypair: Keypair,
        peer_id: PeerId,
        explicit_peers: &[PeerId],
        address: Multiaddr,
        dial_ports: Vec<u16>,
        topic_name: impl Into<String>,
        lapp_service_sender: Sender<LappServiceMessage>,
    ) -> Result<(), Error> {
        let transport = libp2p::tokio_development_transport(keypair.clone())?;
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
        let mut gossipsub_behaviour = gossipsub::Behaviour::new(MessageAuthenticity::Signed(keypair), gossipsub_config)
            .map_err(|err| Error::GossipsubUninit(err.into()))?;

        let topic = Topic::new(topic_name);
        gossipsub_behaviour
            .subscribe(&topic)
            .map_err(Error::GossipsubSubscribtionError)?;
        for peer_id in explicit_peers {
            gossipsub_behaviour.add_explicit_peer(peer_id);
        }

        let behaviour = GossipsubServiceBehaviour {
            gossipsub: gossipsub_behaviour,
            mdns: mdns::tokio::Behaviour::new(mdns::Config::default(), peer_id)?,
        };
        let mut swarm = SwarmBuilder::with_tokio_executor(transport, behaviour, peer_id).build();
        swarm.listen_on(address)?;

        let mut receiver = ctx.actor_receiver::<GossipsubServiceMessage>(actor_id);
        let mut service = Self {
            swarm,
            dial_ports,
            topic,
            lapp_service_sender,
            peers: Default::default(),
        };

        truba::spawn_event_loop!(ctx, {
            event = service.swarm.select_next_some() => match event {
                SwarmEvent::Behaviour(GossipsubServiceBehaviourEvent::Mdns(event)) => service.handle_mdns(event),
                SwarmEvent::Behaviour(GossipsubServiceBehaviourEvent::Gossipsub(event)) => service.handle_gossipsub(event),
                SwarmEvent::NewListenAddr { address, .. } => {
                    log::info!("Local node is listening on {address}");
                }
                SwarmEvent::IncomingConnection {
                    connection_id: _,
                    local_addr,
                    send_back_addr,
                } => log::debug!("Local node incoming connection {local_addr}, {send_back_addr}"),
                _ => {}
            },
            Some(msg) = receiver.recv() => if let Err(err) = service.handle_p2p(msg) {
                log::error!("P2P error for topic \"{}\": {err:?}", service.topic);
            },
        });

        Ok(())
    }

    fn handle_mdns(&mut self, event: mdns::Event) {
        match event {
            mdns::Event::Discovered(peers) => {
                for (peer_id, address) in peers {
                    log::info!("MDNS discovered a new peer: {peer_id} {address}");

                    self.swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                    let addresses = self.peers.entry(peer_id).or_default();
                    if !addresses.contains(&address) {
                        addresses.push(address);
                    }
                }
            },
            mdns::Event::Expired(expired) => {
                for (peer_id, address) in expired {
                    log::info!("MDNS discover peer has expired: {peer_id} {address}");
                    self.swarm.behaviour_mut().gossipsub.remove_explicit_peer(&peer_id);
                    self.peers.remove(&peer_id);
                }
            },
        }
    }

    fn handle_gossipsub(&mut self, event: gossipsub::Event) {
        if let gossipsub::Event::Message {
            propagation_source: peer_id,
            message_id,
            message,
        } = event
        {
            let text = String::from_utf8_lossy(&message.data); // todo: catch error
            log::info!("Got message: {text} with id: {message_id} from peer: {peer_id:?}");
            if message.topic == self.topic.hash() {
                if let Err(err) = self
                    .lapp_service_sender
                    .send(LappServiceMessage::GossipSub(GossipsubServiceMessage(Message::Text {
                        peer_id: peer_id.to_base58(),
                        msg: text.to_string(),
                    })))
                {
                    log::error!("Error occurs when send to lapp service: {err:?}");
                }
            }
        }
    }

    fn handle_p2p(&mut self, GossipsubServiceMessage(msg): GossipsubServiceMessage) -> Result<(), Error> {
        match msg {
            Message::Text { msg, .. } => {
                let topic = self.topic.clone();
                log::info!("Publish message: {msg}");
                self.swarm
                    .behaviour_mut()
                    .gossipsub
                    .publish(topic, msg)
                    .map(drop)
                    .map_err(Error::GossipsubPublishError)
            },
            Message::Dial(peer_id) => {
                log::info!("Dial peer: {peer_id}");
                PeerId::from_str(&peer_id)
                    .map_err(|err| Error::ParsePeerIdError(err.to_string()))
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
            Message::AddAddress(address) => {
                log::info!("Add address: {address}");
                Multiaddr::from_str(&address)
                    .map_err(Error::WrongMultiaddr)
                    .and_then(|address| self.swarm.dial(address).map_err(Error::DialError))
            },
        }
    }
}

pub fn decode_keypair(bytes: &mut [u8]) -> Result<Keypair, Error> {
    Ok(Keypair::from_protobuf_encoding(bytes)?)
}

pub fn decode_peer_id(bytes: &[u8]) -> Result<PeerId, Error> {
    PeerId::from_bytes(bytes).map_err(|err| Error::ParsePeerIdError(err.to_string()))
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
