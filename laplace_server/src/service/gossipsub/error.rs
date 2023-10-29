use std::io;

use laplace_wasm::route::gossipsub::{Error as WasmError, ErrorKind};
use thiserror::Error;

pub type GossipsubResult<T = ()> = Result<T, Error>;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Noise error: {0}")]
    NoiseError(#[from] libp2p::noise::Error),

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

    #[error("Wrong behaviour: {0}")]
    WrongBehaviour(String),

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

impl From<Error> for WasmError {
    fn from(err: Error) -> Self {
        let kind = match &err {
            Error::GossipsubPublishError(_) => ErrorKind::GossipsubPublishError,
            Error::ParsePeerIdError(_) => ErrorKind::ParsePeerIdError,
            Error::DialError(_) => ErrorKind::DialError,
            Error::WrongMultiaddr(_) => ErrorKind::WrongMultiaddr,
            _ => ErrorKind::Other,
        };

        Self {
            message: err.to_string(),
            kind,
        }
    }
}
