pub mod allocator;
pub mod index;
pub mod worker;

pub use allocator::Allocator;
pub use index::Index;

use bon::Builder;
use serde::Deserialize;
use serde::Serialize;

#[derive(Builder, Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    pub config_global: allocator_bench::config::Global,
    pub config_allocator: allocator_bench::allocator::Config,
    pub config_benchmark: allocator_bench::benchmark::Config,

    pub allocator: Allocator,
    pub index: Index,
}

impl Config {
    pub fn with_process_id(&self, process_id: usize) -> worker::Config {
        worker::Config {
            config_process: self.config_global.with_process_id(process_id),
            config_allocator: self.config_allocator,
            config_benchmark: self.config_benchmark.clone(),
            allocator: self.allocator,
            index: self.index,
        }
    }
}

#[derive(Serialize)]
pub struct Observation {
    #[serde(flatten)]
    pub config: Config,

    pub output: allocator_bench::Output,
}
