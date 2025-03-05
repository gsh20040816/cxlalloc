pub mod allocator;
mod benchmark;

pub use allocator::Allocator;
pub use benchmark::Benchmark;

use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Deserialize, Serialize)]
pub struct Cli {
    pub allocator: Allocator,

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
