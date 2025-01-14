use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Serialize, Deserialize)]
pub enum Request {
    Handshake,
    Allocate { id: u64, size: u64 },
    Free { id: u64, offset: u64 },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Response {
    Handshake { socket: String },
    Allocate { offset: u64 },
    Free,
}
