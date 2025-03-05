use core::fmt::Display;

pub mod boost;
pub mod cxl_shm;
pub mod cxlalloc;
pub mod lightning;

pub use boost::Boost;
pub use cxl_shm::CxlShm;
pub use cxlalloc::Cxlalloc;
pub use lightning::Lightning;

use serde::Deserialize;
use serde::Serialize;

use allocator_bench::Backend;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Allocator {
    Boost,
    Cxlalloc,
    CxlShm,
    Lightning,
}

impl Display for Allocator {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Allocator::Boost => "boost",
            Allocator::Cxlalloc => "cxlalloc",
            Allocator::CxlShm => "cxl-shm",
            Allocator::Lightning => "lightning",
        };

        write!(f, "{}", name)
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Cli {
    pub allocator: Allocator,
    pub context: allocator_bench::context::Process,
    pub benchmark: allocator_bench::Benchmark,
}

impl Cli {
    pub fn run<B: Backend>(&self) {
        match &self.benchmark {
            allocator_bench::Benchmark::ThreadTest(thread_test) => {
                <_ as allocator_bench::benchmark::Interface<B>>::run_process(
                    thread_test,
                    &self.context,
                )
            }
            allocator_bench::Benchmark::Ycsb(ycsb) => {
                <_ as allocator_bench::benchmark::Interface<B>>::run_process(ycsb, &self.context)
            }
        }
    }
}
