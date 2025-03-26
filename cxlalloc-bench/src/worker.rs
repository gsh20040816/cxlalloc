use allocator_bench::benchmark;
use allocator_bench::index;
use bon::Builder;
use serde::Deserialize;
use serde::Serialize;

use crate::allocator::boost;
use crate::allocator::cxl_shm;
use crate::allocator::cxlalloc;
use crate::allocator::lightning;
use crate::allocator::mimalloc;
use crate::allocator::ralloc;
use crate::Allocator;
use crate::Index;

#[derive(Builder, Clone, Deserialize, Serialize)]
pub struct Config {
    pub config_process: allocator_bench::config::Process,
    pub config_allocator: allocator_bench::allocator::Config,
    pub config_benchmark: allocator_bench::benchmark::Config,

    pub allocator: Allocator,
    pub index: Index,
}

impl Config {
    pub fn run(&self) {
        self.specialize_allocator()
    }

    fn specialize_allocator(&self) {
        match self.allocator {
            Allocator::Boost => self.specialize_index::<boost::Backend>(),
            Allocator::Cxlalloc => self.specialize_index::<cxlalloc::Backend>(),
            Allocator::CxlShm => self.specialize_index::<cxl_shm::Backend>(),
            Allocator::Lightning => self.specialize_index::<lightning::Backend>(),
            Allocator::Mimalloc => self.specialize_index::<mimalloc::Backend>(),
            Allocator::Ralloc => self.specialize_index::<ralloc::Backend>(),
        }
    }

    fn specialize_index<A: allocator_bench::allocator::Backend>(&self) {
        match self.index {
            Index::Linear => self.specialize_benchmark::<A, index::LinearHashMap>(),
            Index::Linked => self.specialize_benchmark::<A, index::LinkedHashMap>(),
        }
    }

    fn specialize_benchmark<
        B: allocator_bench::allocator::Backend,
        I: allocator_bench::index::Index<B::Allocator, u64>,
    >(
        &self,
    ) {
        match self.config_benchmark.clone() {
            benchmark::Config::Mstress(mstress) => self.run_benchmark::<B, _>(mstress),
            benchmark::Config::ThreadTest(thread_test) => self.run_benchmark::<B, _>(thread_test),
            benchmark::Config::Ycsb(ycsb) => self.run_benchmark::<B, _>(
                allocator_bench::benchmark::Ycsb::<B::Allocator, I>::new(ycsb),
            ),
            benchmark::Config::YcsbLoad(ycsb_load) => self.run_benchmark::<B, _>(
                allocator_bench::benchmark::YcsbLoad::<B::Allocator, I>::new(ycsb_load),
            ),
            benchmark::Config::Xmalloc(xmalloc) => self.run_benchmark::<B, _>(xmalloc),
        }
    }

    fn run_benchmark<A: allocator_bench::allocator::Backend, B: benchmark::Benchmark<A>>(
        &self,
        benchmark: B,
    ) {
        benchmark.run_process(&self.config_process, &self.config_allocator)
    }
}
