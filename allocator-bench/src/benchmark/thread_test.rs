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
    pub(crate) operation_count: u64,

    #[builder(default = 8)]
    pub(crate) object_size: usize,
}

#[derive(Deserialize, Serialize)]
pub struct OutputWorker {
    time: u128,
    operation_count: u64,
    size: u64,
}

impl<B: Backend> benchmark::Benchmark<B> for ThreadTest {
    const NAME: &str = "/tt";
    type StateGlobal = usize;
    type StateProcess = ();
    type StateCoordinator = ();
    type StateWorker = Vec<Option<<B::Allocator as Allocator>::Handle>>;

    type OutputWorker = OutputWorker;
    type OutputCoordinator = ();

    fn setup_global(
        &self,
        config: &config::Process,
        _allocator: &allocator::Config,
    ) -> Self::StateGlobal {
        assert_eq!(
            self.operation_count as usize % config.thread_count,
            0,
            "Object count should be multiple of total thread count"
        );

        self.operation_count as usize / config.thread_count
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

        let time = start.elapsed().as_nanos();
        let operation_count = handles.len() as u64 * self.iteration_count;
        OutputWorker {
            time,
            operation_count,
            size: operation_count * self.object_size as u64,
        }
    }
}
