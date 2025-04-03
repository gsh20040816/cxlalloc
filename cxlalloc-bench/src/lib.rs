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
    pub global: allocator_bench::config::Global,
    allocator: allocator_bench::allocator::Config<serde_json::Value>,
    benchmark: allocator_bench::benchmark::Config,
}

impl Config {
    pub fn with_process_id(&self, process_id: usize) -> worker::Config {
        worker::Config {
            process: self.global.with_process_id(process_id),
            allocator: self.allocator.clone(),
            benchmark: self.benchmark.clone(),
        }
    }
}
