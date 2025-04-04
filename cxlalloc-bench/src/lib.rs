pub mod allocator;
pub mod index;
pub mod worker;

use std::time::SystemTime;
use std::time::UNIX_EPOCH;

pub use allocator::Allocator;
pub use index::Index;

use bon::Builder;
use serde::Deserialize;
use serde::Serialize;

#[derive(Builder, Clone, Debug, Deserialize, Serialize)]
#[builder(state_mod(name = "config", vis = "pub"), derive(Clone, Debug))]
pub struct Config {
    #[builder(default = date())]
    date: u64,
    pub global: allocator_bench::config::Global,
    allocator: allocator_bench::allocator::Config<serde_json::Value>,
    benchmark: allocator_bench::benchmark::Config,
}

impl Config {
    pub fn with_process_id(&self, process_id: usize) -> worker::Config {
        worker::Config {
            date: self.date,
            process: self.global.with_process_id(process_id),
            allocator: self.allocator.clone(),
            benchmark: self.benchmark.clone(),
        }
    }
}

fn date() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}
