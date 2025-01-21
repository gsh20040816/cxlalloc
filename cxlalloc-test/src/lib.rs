pub mod trace;

use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Request {
    Handshake,
    Allocate { id: u64, size: u64 },
    Free { id: u64, size: u64, offset: u64 },
    Load { id: u64, offset: u64 },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Response {
    Handshake { socket: String },
    Allocate { offset: u64 },
    Load { value: u64 },
    Free,
}
