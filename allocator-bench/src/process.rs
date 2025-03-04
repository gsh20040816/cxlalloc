use serde::Deserialize;
use serde::Serialize;

use crate::Benchmark;
use crate::context;

#[derive(Clone, Deserialize, Serialize)]
pub struct Cli {
    pub context: context::Process,
    pub benchmark: Benchmark,
}
