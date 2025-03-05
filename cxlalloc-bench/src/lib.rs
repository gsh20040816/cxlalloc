mod allocator;
mod benchmark;
pub mod process;

pub use allocator::Allocator;
pub use benchmark::Benchmark;

use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Deserialize, Serialize)]
pub struct Cli {
    pub allocator: process::Allocator,

    #[serde(flatten)]
    pub control: allocator_bench::context::Global,

    #[serde(flatten)]
    pub benchmark: allocator_bench::Benchmark,
}

#[derive(Serialize)]
pub struct Observation {
    #[serde(flatten)]
    pub inputs: Cli,

    #[serde(flatten)]
    pub outputs: allocator_bench::Metrics,
}
