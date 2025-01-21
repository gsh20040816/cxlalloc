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
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Request {
    Allocate { thread: u64, id: u64, size: u64 },
    Free { thread: u64, id: u64 },
    Load { thread: u64, id: u64 },
}
