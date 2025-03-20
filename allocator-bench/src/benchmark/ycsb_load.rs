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
use crate::allocator::Handle as _;
use crate::benchmark;
use crate::config;
use crate::index;

#[derive(Builder, Clone, Debug, Deserialize, Serialize)]
pub struct YcsbLoad {
    index: index::Config,

    /// Whether to write value
    write: bool,

    #[serde(flatten)]
    workload: ycsb::Workload,
}

// HACK: CXL-SHM doesn't support allocations larger than 1KiB (1_000B data + 24B header)
#[expect(unused)]
const MAX_SIZE: usize = 1_000;

pub struct Global<I> {
    index: I,
}

pub(super) struct Record(pub(super) [Field; 10]);

#[repr(C)]
pub(super) struct Field {
    pub(super) value: [AtomicU8; 96],
}

unsafe impl<I> Sync for Global<I> {}

#[derive(Serialize)]
pub struct Output {
    time: u128,
}

impl<B: Backend, I: Index<B::Allocator>> benchmark::Benchmark<B, I> for YcsbLoad {
    const NAME: &str = "ycsb";
    type StateGlobal = Global<I>;

    type StateCoordinator = ();
    type StateWorker = ();

    type OutputWorker = u128;
    type OutputCoordinator = ();
    type OutputGlobal = Output;

    fn setup_process(
        &self,
        _config: &config::Process,
        allocator: &allocator::Config,
    ) -> Self::StateGlobal {
        Global {
            index: I::new(
                Some(allocator.numa),
                "index",
                self.index.len,
                self.index.populate,
            )
            .unwrap(),
        }
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
        _global: &Self::StateGlobal,
        _allocator: &mut B::Allocator,
    ) -> Self::StateWorker {
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
        config: &config::Thread,
        global: &Self::StateGlobal,
        _worker: &mut Self::StateWorker,
        allocator: &mut B::Allocator,
    ) -> Self::OutputWorker {
        let start = Instant::now();
        match self.index.inline {
            true => {
                load::<true, _, _>(self.write, &self.workload, config, allocator, &global.index)
            }
            false => {
                load::<false, _, _>(self.write, &self.workload, config, allocator, &global.index)
            }
        }
        start.elapsed().as_nanos()
    }

    fn teardown_process(&self, config: &config::Process, mut global: Self::StateGlobal) {
        if config.process_id != 0 {
            return;
        }

        global.index.unlink().unwrap();
    }

    fn aggregate(
        (): Self::OutputCoordinator,
        workers: Vec<Self::OutputWorker>,
    ) -> Self::OutputGlobal {
        let total = workers.iter().sum::<u128>();
        let time = total / workers.len() as u128;
        Output { time }
    }
}

pub(super) fn load<const INLINE: bool, A: Allocator, I: Index<A>>(
    write: bool,
    workload: &ycsb::Workload,
    config: &config::Thread,
    allocator: &mut A,
    index: &I,
) {
    let mut loader = workload.loader(config.thread_total(), config.thread_id);

    while let Some(key) = loader.next_key() {
        insert::<INLINE, _, _>(write, allocator, index, &key);
    }
}

pub(super) fn insert<const INLINE: bool, A: Allocator, I: Index<A>>(
    write: bool,
    allocator: &mut A,
    index: &I,
    key: &ycsb::Key,
) {
    const SIZE: usize = mem::size_of::<Record>();
    match INLINE {
        true => index.insert(allocator, key.id(), SIZE, |_, pointer| {
            if write {
                unsafe {
                    libc::memset(pointer.cast(), 0xff, SIZE);
                }
            }
        }),
        false => {
            let value = allocator.allocate(SIZE).unwrap();

            if write {
                unsafe {
                    libc::memset(value.as_ptr(), 0xff, SIZE);
                }
            }

            index.insert(
                allocator,
                key.id(),
                mem::size_of::<u64>(),
                |allocator, pointer| unsafe {
                    allocator.link(pointer.cast(), &value);
                },
            );
        }
    }
}
