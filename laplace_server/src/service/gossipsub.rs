use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::ops::ControlFlow;
use std::str::FromStr;
use std::time::Duration;

pub use laplace_wasm::route::gossipsub::{Message, MessageIn, MessageOut};
use libp2p::futures::StreamExt;
use libp2p::gossipsub::{self, IdentTopic as Topic, MessageAuthenticity, MessageId, ValidationMode};
use libp2p::identity::Keypair;
use libp2p::multiaddr::Protocol;
use libp2p::swarm::{NetworkBehaviour, SwarmEvent};
use libp2p::{mdns, noise, tcp, yamux, Multiaddr, PeerId, Swarm, SwarmBuilder};
use truba::{Context, Sender, UnboundedMpscChannel};

pub use crate::service::gossipsub::error::{Error, GossipsubResult};
use crate::service::lapp::LappServiceMessage;
use crate::service::Addr;

pub mod error;

#[derive(Debug)]
pub struct GossipsubServiceMessage(pub MessageOut);

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
    ) -> GossipsubResult {
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

        let behaviour = GossipsubServiceBehaviour {
            gossipsub: gossipsub::Behaviour::new(MessageAuthenticity::Signed(keypair.clone()), gossipsub_config)
                .map_err(|err| Error::GossipsubUninit(err.into()))?,
            mdns: mdns::tokio::Behaviour::new(mdns::Config::default(), peer_id)?,
        };

        let mut swarm = SwarmBuilder::with_existing_identity(keypair)
            .with_tokio()
            .with_tcp(tcp::Config::default(), noise::Config::new, yamux::Config::default)?
            .with_behaviour(|_keypair| Ok(behaviour))
            .map_err(|err| Error::WrongBehaviour(err.to_string()))?
            .build();

        let topic = Topic::new(topic_name);
        swarm
            .behaviour_mut()
            .gossipsub
            .subscribe(&topic)
            .map_err(Error::GossipsubSubscribtionError)?;
        for peer_id in explicit_peers {
            swarm.behaviour_mut().gossipsub.add_explicit_peer(peer_id);
        }

        swarm.listen_on(address)?;

        let mut service_message_in = ctx.actor_receiver::<GossipsubServiceMessage>(actor_id);
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
                },
                SwarmEvent::IncomingConnection {
                    connection_id: _,
                    local_addr,
                    send_back_addr,
                } => log::debug!("Local node incoming connection {local_addr}, {send_back_addr}"),
                _ => {},
            },
            Some(GossipsubServiceMessage(MessageOut { id, msg })) = service_message_in.recv() => {
                let result = service.handle_p2p(msg);
                let is_break = match &result {
                    Ok(ControlFlow::Break(_)) => true,
                    Err(err) => {
                        log::error!("P2P error for topic \"{}\": {err:?}", service.topic);
                        false
                    }
                    _ => false,
                };
                service.send_to_lapp(MessageIn::Response { id, result: result.map(drop).map_err(Into::into) });

                if is_break { break }
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
            log::debug!("Got message: {text} with id: {message_id} from peer: {peer_id:?}");
            if message.topic == self.topic.hash() {
                self.send_to_lapp(MessageIn::Text {
                    peer_id: peer_id.to_base58(),
                    msg: text.to_string(),
                });
            }
        }
    }

    fn handle_p2p(&mut self, msg: Message) -> GossipsubResult<ControlFlow<()>> {
        match msg {
            Message::Text { msg, .. } => {
                let topic = self.topic.clone();
                log::debug!("Publish message: {msg}");
                self.swarm
                    .behaviour_mut()
                    .gossipsub
                    .publish(topic, msg)
                    .map(drop)
                    .map(ControlFlow::Continue)
                    .map_err(Error::GossipsubPublishError)
            },
            Message::Dial(peer_id) => {
                log::debug!("Dial peer: {peer_id}");
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
                                log::debug!("Dial address: {address}");
                                self.swarm.dial(address.clone()).map_err(Error::DialError)?;
                            }
                            Ok(ControlFlow::Continue(()))
                        } else {
                            Err(Error::DialError(libp2p::swarm::DialError::NoAddresses))
                        }
                    })
            },
            Message::AddAddress(address) => {
                log::debug!("Add address: {address}");
                Multiaddr::from_str(&address)
                    .map_err(Error::WrongMultiaddr)
                    .and_then(|address| self.swarm.dial(address).map_err(Error::DialError))
                    .map(ControlFlow::Continue)
            },
            Message::Close => {
                log::debug!("Closing gossipsub service");
                Ok(ControlFlow::Break(()))
            },
        }
    }

    fn send_to_lapp(&self, msg: MessageIn) {
        if let Err(err) = self.lapp_service_sender.send(LappServiceMessage::Gossipsub(msg)) {
            log::error!("Error occurs when send to lapp service: {err:?}");
        }
    }
}

pub fn decode_keypair(bytes: &mut [u8]) -> GossipsubResult<Keypair> {
    Ok(Keypair::from_protobuf_encoding(bytes)?)
}

pub fn decode_peer_id(bytes: &[u8]) -> GossipsubResult<PeerId> {
    PeerId::from_bytes(bytes).map_err(|err| Error::ParsePeerIdError(err.to_string()))
}
