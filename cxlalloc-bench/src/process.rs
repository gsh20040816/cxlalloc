use core::fmt::Display;

use allocator_bench::Backend;
use clap::Parser;
use clap::ValueEnum;

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

#[derive(Clone, ValueEnum, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
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

#[derive(Clone, Parser, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
#[group(skip)]
pub struct Cli {
    #[arg(short, long)]
    pub allocator: Allocator,

    #[arg(short, long)]
    pub size: usize,

    #[command(flatten)]
    pub benchmark: allocator_bench::process::Cli,
}

impl Cli {
    pub fn run<B: Backend>(&self) {
        match &self.benchmark.benchmark {
            allocator_bench::Benchmark::ThreadTest(thread_test) => {
                <_ as allocator_bench::benchmark::Interface<B>>::run_process(
                    thread_test,
                    &self.benchmark.context,
                    self.size,
                )
            }
            allocator_bench::Benchmark::Ycsb(ycsb) => {
                <_ as allocator_bench::benchmark::Interface<B>>::run_process(
                    ycsb,
                    &self.benchmark.context,
                    self.size,
                )
            }
        }
    }
}
