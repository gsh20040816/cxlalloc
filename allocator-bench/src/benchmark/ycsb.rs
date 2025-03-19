use core::mem;
use core::sync::atomic::AtomicU8;
use core::sync::atomic::Ordering;
use std::time::Instant;

use bon::Builder;
use serde::Deserialize;
use serde::Serialize;
use shm::Shm;

use crate::Allocator;
use crate::Index;
use crate::allocator;
use crate::allocator::Backend;
use crate::allocator::Handle as _;
use crate::benchmark;
use crate::config;
use crate::index;

#[derive(Builder, Clone, Debug, Deserialize, Serialize)]
pub struct Ycsb {
    /// Whether to measure loading only (or else running phase only)
    load: bool,

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
    acked: Shm<ycsb::Acknowledged>,
}

struct Record([Field; 10]);

#[derive(Serialize)]
pub struct Data {
    time: u128,
}

#[repr(C)]
struct Field {
    value: [AtomicU8; 96],
}

unsafe impl<I> Sync for Global<I> {}

impl<B: Backend, I: Index<B::Allocator>> benchmark::Benchmark<B, I> for Ycsb {
    const NAME: &str = "ycsb";
    type Global = Global<I>;
    type Local = ();
    type Data = Data;

    fn setup_process(&self, _config: &config::Process, allocator: &allocator::Config) -> Global<I> {
        Global {
            index: I::new(
                Some(allocator.numa),
                "index",
                self.index.len,
                self.index.populate,
            )
            .unwrap(),
            acked: Shm::new(None, c"acked".to_owned(), true).unwrap(),
        }
    }

    fn setup_thread(
        &self,
        config: &config::Thread,
        global: &Self::Global,
        allocator: &mut B::Allocator,
    ) -> Self::Local {
        if self.load {
            return;
        }

        match self.index.inline {
            true => {
                load::<true, _, _>(self.write, &self.workload, config, allocator, &global.index)
            }
            false => {
                load::<false, _, _>(self.write, &self.workload, config, allocator, &global.index)
            }
        }
    }

    fn run_thread(
        &self,
        config: &config::Thread,
        global: &Self::Global,
        _local: &mut Self::Local,
        allocator: &mut B::Allocator,
    ) -> Self::Data {
        if self.load {
            let start = Instant::now();
            match self.index.inline {
                true => {
                    load::<true, _, _>(self.write, &self.workload, config, allocator, &global.index)
                }
                false => load::<false, _, _>(
                    self.write,
                    &self.workload,
                    config,
                    allocator,
                    &global.index,
                ),
            }

            return Data {
                time: start.elapsed().as_micros(),
            };
        }

        let mut runner = self
            .workload
            .runner(unsafe { global.acked.address().as_ref().unwrap() });

        let start = Instant::now();
        match self.index.inline {
            true => run::<true, _, _>(
                self.write,
                self.workload.operation_count(),
                config,
                &mut runner,
                allocator,
                &global.index,
            ),
            false => run::<false, _, _>(
                self.write,
                self.workload.operation_count(),
                config,
                &mut runner,
                allocator,
                &global.index,
            ),
        }

        Data {
            time: start.elapsed().as_micros(),
        }
    }

    fn teardown_process(&self, config: &config::Process, mut global: Self::Global) {
        if config.process_id != 0 {
            return;
        }

        global.index.unlink().unwrap();
        global.acked.unlink().unwrap();
    }
}

fn load<const INLINE: bool, A: Allocator, I: Index<A>>(
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

fn run<const INLINE: bool, A: Allocator, I: Index<A>>(
    write: bool,
    operation_count: usize,
    config: &config::Thread,
    runner: &mut ycsb::Runner,
    allocator: &mut A,
    index: &I,
) {
    let mut rng = rand::rng();

    for _ in 0..operation_count / config.thread_total() {
        let key = runner.next_key(&mut rng);
        match runner.next_operation(&mut rng) {
            ycsb::Operation::Read => {
                with::<INLINE, _, _, _>(allocator, index, &key, |value| unsafe {
                    let record = value.cast::<Record>().as_ref().unwrap();
                    for field in &record.0 {
                        (field as *const Field).read_volatile();
                    }
                })
            }
            ycsb::Operation::Update => {
                let field = runner.next_field(&mut rng);
                with::<INLINE, _, _, _>(allocator, index, &key, |value| unsafe {
                    let record = value.cast::<Record>().as_ref().unwrap();
                    record.0[field as usize].value[0].store(1, Ordering::Release);
                });
            }
            ycsb::Operation::Scan => todo!(),
            ycsb::Operation::Insert => {
                insert::<INLINE, _, _>(write, allocator, index, &key);
            }
            ycsb::Operation::ReadModifyWrite => todo!(),
        }
    }
}

fn insert<const INLINE: bool, A: Allocator, I: Index<A>>(
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

fn with<const INLINE: bool, A: Allocator, I: Index<A>, F: FnOnce(*const u8)>(
    allocator: &mut A,
    index: &I,
    key: &ycsb::Key,
    with: F,
) {
    match INLINE {
        true => {
            let found = index.get(allocator, key.id(), |_, value| with(value));
            assert!(found);
        }
        false => {
            let found = index.get(allocator, key.id(), |allocator, pointer| {
                let offset = unsafe { pointer.cast::<u64>().read() };
                let handle = allocator.offset_to_handle(offset).unwrap();
                with(handle.as_ptr().cast())
            });
            assert!(found);
        }
    }
}
