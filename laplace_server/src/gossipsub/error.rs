use std::io;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Hash error: {0}")]
    HashError(#[from] libp2p::multihash::Error),

    #[error("Fail identity decode: {0}")]
    IdentityDecodeError(#[from] libp2p::identity::error::DecodingError),

    #[error("Wrong multiaddr: {0}")]
    WrongMultiaddr(#[from] libp2p::multiaddr::Error),

    #[error("Dial error: {0}")]
    DialError(#[from] libp2p::swarm::DialError),

    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("Gossipsub uninitialize: {0}")]
    GossipsubUninit(String),

    #[error("Gossipsub subscription error: {0:?}")]
    GossipsubSubscribtionError(libp2p::gossipsub::error::SubscriptionError),

    #[error("Gossipsub publish error: {0:?}")]
    GossipsubPublishError(libp2p::gossipsub::error::PublishError),

    #[error("Parse peer ID error: {0}")]
    ParsePeerIdError(String),

    #[error("Transport error: {0}")]
    TransportError(#[from] libp2p::TransportError<io::Error>),
}
