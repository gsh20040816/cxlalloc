use allocator_bench::benchmark;
use allocator_bench::index;
use serde::Deserialize;
use serde::Serialize;

use crate::allocator::boost;
use crate::allocator::cxl_shm;
use crate::allocator::cxlalloc;
use crate::allocator::lightning;
use crate::Allocator;
use crate::Index;

#[derive(Clone, Deserialize, Serialize)]
pub struct Cli {
    pub allocator: Allocator,
    pub index: Index,
    pub context: allocator_bench::context::Process,
    pub benchmark: allocator_bench::Benchmark,
}

impl Cli {
    pub fn run(&self) {
        self.specialize_allocator()
    }

    fn specialize_allocator(&self) {
        match self.allocator {
            Allocator::Boost => self.specialize_index::<boost::Backend>(),
            Allocator::Cxlalloc => self.specialize_index::<cxlalloc::Backend>(),
            Allocator::CxlShm => self.specialize_index::<cxl_shm::Backend>(),
            Allocator::Lightning => self.specialize_index::<lightning::Backend>(),
        }
    }

    fn specialize_index<A: allocator_bench::allocator::Backend>(&self) {
        match self.index {
            Index::Linear => self.specialize_benchmark::<A, index::LinearHashMap>(),
            Index::Linked => self.specialize_benchmark::<A, index::LinkedHashMap>(),
        }
    }

    fn specialize_benchmark<
        A: allocator_bench::allocator::Backend,
        I: allocator_bench::index::Index<A::Allocator>,
    >(
        &self,
    ) {
        match self.benchmark.clone() {
            benchmark::Benchmark::ThreadTest(thread_test) => {
                self.run_benchmark::<A, I, _>(thread_test)
            }
            benchmark::Benchmark::Ycsb(ycsb) => self.run_benchmark::<A, I, _>(ycsb),
        }
    }

    fn run_benchmark<
        A: allocator_bench::allocator::Backend,
        I: allocator_bench::index::Index<A::Allocator>,
        B: benchmark::Interface<A, I>,
    >(
        &self,
        benchmark: B,
    ) {
        benchmark.run_process(&self.context)
    }
}
