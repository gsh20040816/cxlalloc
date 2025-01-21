use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Test {
    /// Number of threads
    pub count: usize,

    /// Initial heap size
    pub size: usize,

    pub requests: Vec<Request>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Request {
    Handshake { thread: u64 },
    Allocate { thread: u64, id: u64, size: u64 },
    Free { thread: u64, id: u64 },
    Load { thread: u64, id: u64 },
}

impl Request {
    pub fn thread(&self) -> u64 {
        match self {
            Request::Handshake { thread }
            | Request::Allocate { thread, .. }
            | Request::Free { thread, .. }
            | Request::Load { thread, .. } => *thread,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Response {
    Handshake { socket: String },
    Allocate { offset: u64 },
    Load { value: u64 },
    Free,
}
