// https://github.com/emeryberger/Hoard/blob/f021bdb810332c9c9f5a11ae5404aaa38fe129c0/benchmarks/threadtest/threadtest.cpp

use std::time::Instant;

use bon::Builder;
use serde::Deserialize;
use serde::Serialize;

use crate::Allocator;
use crate::Index;
use crate::allocator;
use crate::allocator::Backend;
use crate::benchmark;
use crate::config;

#[derive(Builder, Clone, Debug, Deserialize, Serialize)]
pub struct ThreadTest {
    #[builder(default = 50)]
    pub(crate) iteration_count: usize,

    #[builder(default = 30_000)]
    pub(crate) object_count: usize,

    #[builder(default = 8)]
    pub(crate) object_size: usize,
}

#[derive(Serialize)]
pub struct Output {
    time: u128,
}

impl<B: Backend, I: Index<B::Allocator>> benchmark::Benchmark<B, I> for ThreadTest {
    const NAME: &str = "tt";
    type StateGlobal = usize;

    type StateCoordinator = ();
    type StateWorker = Vec<Option<<B::Allocator as Allocator>::Handle>>;

    type OutputWorker = Output;
    type OutputCoordinator = ();
    type OutputGlobal = Output;

    fn setup_process(
        &self,
        config: &config::Process,
        _allocator: &allocator::Config,
    ) -> Self::StateGlobal {
        let thread_total = config.thread_total();
        assert_eq!(
            self.object_count % thread_total,
            0,
            "Object count should be multiple of total thread count"
        );

        self.object_count / thread_total
    }

    fn setup_coordinator(
        &self,
        _config: &config::Process,
        _global: &Self::StateGlobal,
    ) -> Self::StateCoordinator {
    }

    fn setup_worker(
        &self,
        _config: &config::Thread,
        object_count: &Self::StateGlobal,
        _allocator: &mut B::Allocator,
    ) -> Self::StateWorker {
        (0..*object_count).map(|_| None).collect()
    }

    fn run_coordinator(
        &self,
        _config: &config::Process,
        _global: &Self::StateGlobal,
        _coordinator: &mut Self::StateCoordinator,
    ) -> Self::OutputCoordinator {
    }

    fn run_worker(
        &self,
        _config: &config::Thread,
        _: &Self::StateGlobal,
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

        Output {
            time: start.elapsed().as_micros(),
        }
    }

    fn aggregate(
        (): Self::OutputCoordinator,
        workers: Vec<Self::OutputWorker>,
    ) -> Self::OutputGlobal {
        let total = workers.iter().map(|Output { time }| *time).sum::<u128>();
        Output {
            time: total / workers.len() as u128,
        }
    }
}
