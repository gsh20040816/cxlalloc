use core::mem;
use core::sync::atomic::AtomicU8;
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
use crate::index;

#[derive(Builder, Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    pub index: index::Config,

    #[serde(flatten)]
    workload: ycsb::Workload,
}

pub struct Global<I> {
    index: I,
}

pub(super) struct Record(pub(super) [Field; 10]);

#[repr(C)]
pub(super) struct Field {
    pub(super) value: [AtomicU8; 96],
}

unsafe impl<I> Sync for Global<I> {}

#[derive(Deserialize, Serialize)]
pub struct OutputWorker {
    time: u128,
    operation_count: u64,
}

impl<B: Backend, I: Index<B::Allocator>> benchmark::Benchmark<B> for index::Capture<Config, I> {
    const NAME: &str = "/ycsb-load";
    type StateGlobal = Global<I>;
    type StateProcess = ();
    type StateCoordinator = ();
    type StateWorker = ();

    type OutputWorker = OutputWorker;
    type OutputCoordinator = ();

    fn setup_global(
        &self,
        config: &config::Process,
        allocator: &allocator::Config<B::Config>,
    ) -> Self::StateGlobal {
        assert_eq!(
            self.workload.operation_count % config.thread_count,
            0,
            "Operation count {} must be evenly divisible by thread count {}",
            self.workload.operation_count,
            config.thread_count,
        );

        Global {
            index: I::new(
                Some(allocator.numa),
                self.index.len,
                config.is_leader(),
                self.index.populate,
                config.thread_count,
            )
            .unwrap(),
        }
    }

    fn setup_process(
        &self,
        _config: &config::Process,
        _allocator: &allocator::Config<B::Config>,
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
        _global: &Self::StateGlobal,
        (): &Self::StateProcess,
        _allocator: &mut B::Allocator,
    ) -> Self::StateWorker {
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
        config: &config::Thread,
        global: &Self::StateGlobal,
        (): &Self::StateProcess,
        _worker: &mut Self::StateWorker,
        allocator: &mut B::Allocator,
    ) -> Self::OutputWorker {
        let start = Instant::now();
        load(&self.workload, config, allocator, &global.index);
        let time = start.elapsed().as_nanos();
        OutputWorker {
            time,
            operation_count: (self.workload.operation_count / config.thread_count) as u64,
        }
    }

    fn teardown_global(&self, config: &config::Process, mut global: Self::StateGlobal) {
        if !config.is_leader() {
            return;
        }

        global.index.unlink().unwrap();
    }
}

pub(super) fn load<A: Allocator, I: Index<A>>(
    workload: &ycsb::Workload,
    config: &config::Thread,
    allocator: &mut A,
    index: &I,
) {
    let mut loader = workload.loader(config.thread_count, config.thread_id);

    while let Some(key) = loader.next_key() {
        insert::<_, _>(config.thread_id, allocator, index, &key);
    }
}

pub(super) fn insert<A: Allocator, I: Index<A>>(
    thread_id: usize,
    allocator: &mut A,
    index: &I,
    key: &ycsb::Key,
) {
    const SIZE: usize = mem::size_of::<Record>();
    index.insert(
        thread_id,
        allocator,
        &key.id().to_ne_bytes(),
        SIZE,
        |pointer| unsafe {
            libc::memset(pointer.cast(), 0xff, SIZE);
        },
    );
}
