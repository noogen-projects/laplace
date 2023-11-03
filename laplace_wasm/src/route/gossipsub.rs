use borsh::{BorshDeserialize, BorshSerialize};

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub enum MessageIn {
    Text { peer_id: String, msg: String },
    Response { id: String, result: Result<(), Error> },
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct MessageOut {
    pub id: String,
    pub msg: Message,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub enum Message {
    Dial(String),
    AddAddress(String),
    Text { peer_id: String, msg: String },
    Close,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct Error {
    pub message: String,
    pub kind: ErrorKind,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub enum ErrorKind {
    GossipsubPublishError,
    ParsePeerIdError,
    DialError,
    WrongMultiaddr,
    Other,
}
