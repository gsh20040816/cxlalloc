// https://github.com/emeryberger/Hoard/blob/f021bdb810332c9c9f5a11ae5404aaa38fe129c0/benchmarks/threadtest/threadtest.cpp

use std::time::Instant;

use bon::Builder;
use serde::Deserialize;
use serde::Serialize;

use crate::Allocator;
use crate::allocator;
use crate::allocator::Backend;
use crate::benchmark;
use crate::config;

#[derive(Builder, Clone, Debug, Deserialize, Serialize)]
pub struct ThreadTest {
    #[builder(default = 100)]
    pub(crate) iteration_count: u64,

    #[builder(default = 100_000)]
    pub(crate) object_count: u64,

    #[builder(default = 8)]
    pub(crate) object_size: usize,
}

#[derive(Serialize)]
pub struct Output {
    time: u128,
    throughput: u64,
}

impl<B: Backend> benchmark::Benchmark<B> for ThreadTest {
    const NAME: &str = "/tt";
    type StateGlobal = usize;
    type StateProcess = ();
    type StateCoordinator = ();
    type StateWorker = Vec<Option<<B::Allocator as Allocator>::Handle>>;

    type OutputWorker = u128;
    type OutputCoordinator = u64;
    type OutputProcess = Output;

    fn setup_global(
        &self,
        config: &config::Process,
        _allocator: &allocator::Config,
    ) -> Self::StateGlobal {
        assert_eq!(
            self.object_count as usize % config.thread_count,
            0,
            "Object count should be multiple of total thread count"
        );

        self.object_count as usize / config.thread_count
    }

    fn setup_process(
        &self,
        _config: &config::Process,
        _allocator: &allocator::Config,
    ) -> Self::StateProcess {
    }

    fn setup_coordinator(
        &self,
        _config: &config::Process,
        _global: &Self::StateGlobal,
        (): &Self::StateProcess,
    ) -> Self::StateCoordinator {
    }

    fn setup_worker(
        &self,
        _config: &config::Thread,
        object_count: &Self::StateGlobal,
        (): &Self::StateProcess,
        _allocator: &mut B::Allocator,
    ) -> Self::StateWorker {
        (0..*object_count).map(|_| None).collect()
    }

    fn run_coordinator(
        &self,
        _config: &config::Process,
        _global: &Self::StateGlobal,
        (): &Self::StateProcess,
        _coordinator: &mut Self::StateCoordinator,
    ) -> Self::OutputCoordinator {
        self.object_count * self.iteration_count * 2
    }

    fn run_worker(
        &self,
        _config: &config::Thread,
        _: &Self::StateGlobal,
        (): &Self::StateProcess,
        handles: &mut Self::StateWorker,
        allocator: &mut B::Allocator,
    ) -> Self::OutputWorker {
        let start = Instant::now();

        for _ in 0..self.iteration_count {
            for handle in &mut *handles {
                *handle = allocator.allocate(self.object_size);
            }

            for handle in &mut *handles {
                let handle = handle.take().unwrap();
                unsafe {
                    allocator.deallocate(handle);
                }
            }
        }

        start.elapsed().as_nanos()
    }

    fn aggregate(
        count: Self::OutputCoordinator,
        workers: Vec<Self::OutputWorker>,
    ) -> Self::OutputProcess {
        let total = workers.iter().copied().sum::<u128>();
        let time = total / workers.len() as u128;
        let throughput = (count as f64 / time as f64) * 1e9;
        Output {
            time,
            throughput: throughput as u64,
        }
    }
}
