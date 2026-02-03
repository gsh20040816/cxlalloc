use core::sync::atomic::Ordering;
use std::ffi::OsStr;
use std::io;
use std::io::Write as _;
use std::sync::Barrier;
use std::time::Instant;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use crate::clevel;
use crate::queue;
use crate::BARRIER;
use crate::BLOCK;
use crate::CACHE_COUNT;
use crate::CACHE_SIZE;
use crate::CRASH;
use crate::CRASH_COUNT;
use crate::CRASH_VICTIM;
use crate::FINAL;
use crate::GLOBAL;
use crate::OBJECT_COUNT;
use crate::THREAD_COUNT;
use bon::Builder;
use clap::ValueEnum;
use memento::ds::clevel::Clevel;

use memento::ds::queue::Queue;
use memento::pmem::PAllocator as _;
use memento::pmem::PMEMAllocator;
use memento::pmem::PPtr;
use memento::pmem::Pool;
use serde::Deserialize;
use serde::Serialize;

#[derive(Builder, Deserialize, Serialize)]
pub struct Config {
    #[serde(skip_deserializing)]
    allocator: Allocator,

    /// Crash this thread
    crash_victim: Option<usize>,

    crash_count: u64,

    /// Block for garbage collection
    block: bool,

    object_count: u64,

    thread_count: u64,

    /// Heap size
    heap_size: usize,

    workload: Workload,
}

#[derive(Clone, ValueEnum, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Allocator {
    Cxlalloc,
    Ralloc,
}

impl Default for Allocator {
    fn default() -> Self {
        if cfg!(feature = "cxlalloc") {
            Allocator::Cxlalloc
        } else {
            Allocator::Ralloc
        }
    }
}

#[derive(Clone, ValueEnum, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Workload {
    Queue,
    Clevel,
}

#[derive(Serialize)]
pub struct Experiment {
    config: Config,
    output: Output,
}

#[derive(Serialize)]
pub struct Output {
    time: u128,
    date: u64,
    gc_time: usize,
    gc_count: usize,
    cache_count: usize,
    cache_size: usize,
}

// FIXME: make these two consistent
// memento's ralloc uses former (open vs. shm_open)
const PATH: &str = "/dev/shm/pool";
const NAME: &str = "/pool";

impl Config {
    pub fn run(self) {
        if let Some(thread) = self.crash_victim {
            CRASH_VICTIM.store(thread, Ordering::Relaxed);
        }

        THREAD_COUNT.store(self.thread_count, Ordering::Relaxed);
        OBJECT_COUNT.store(self.object_count, Ordering::Relaxed);
        CRASH_COUNT.store(self.crash_count, Ordering::Relaxed);
        CRASH.store(
            match self.crash_count {
                0 => u64::MAX,
                crash_count => self.object_count / (crash_count + 1),
            },
            Ordering::Relaxed,
        );

        BLOCK.store(self.block, Ordering::Relaxed);

        let time;

        unlink(NAME).unwrap();

        match self.workload {
            Workload::Queue => {
                FINAL.store(
                    queue::sum(self.object_count) * self.thread_count,
                    Ordering::Relaxed,
                );
                BARRIER.get_or_init(|| Barrier::new(self.thread_count as usize));

                let pool = Pool::create::<Queue<PPtr<u64>>, queue::Mmt>(
                    PATH,
                    self.heap_size,
                    self.thread_count as usize,
                )
                .unwrap();

                let start = Instant::now();
                pool.execute::<Queue<PPtr<u64>>, queue::Mmt>();
                time = start.elapsed();
                assert_eq!(
                    GLOBAL.load(Ordering::Relaxed),
                    FINAL.load(Ordering::Relaxed),
                );
            }
            Workload::Clevel => {
                BARRIER.get_or_init(|| Barrier::new(self.thread_count as usize - 1));

                // https://github.com/kaist-cp/memento/blob/b88835a7c2e62d2d7f4057ca119f584cc39a1d22/src/ds/clevel.rs#L2141-L2150
                let (send, recv) = crossbeam_channel::bounded(8);
                unsafe {
                    clevel::SEND = Some(core::array::from_fn(|_| None));
                    #[expect(static_mut_refs)]
                    for i in (2..).take(self.thread_count as usize - 1) {
                        clevel::SEND.as_mut().unwrap()[i] = Some(send.clone());
                    }
                    clevel::RECV = Some(recv);
                    drop(send);
                }

                let pool = Pool::create::<Clevel<u64, PPtr<u64>>, clevel::Mmt>(
                    PATH,
                    self.heap_size,
                    self.thread_count as usize,
                )
                .unwrap();

                let start = Instant::now();
                pool.execute::<Clevel<u64, PPtr<u64>>, clevel::Mmt>();
                time = start.elapsed();
            }
        }

        unlink(NAME).unwrap();

        let output = Output {
            time: time.as_micros(),
            date: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            cache_count: CACHE_COUNT.load(Ordering::Relaxed),
            cache_size: CACHE_SIZE.load(Ordering::Relaxed),
            gc_count: unsafe { PMEMAllocator::gc_count() },
            gc_time: unsafe { PMEMAllocator::gc_time() },
        };

        let mut stdout = io::stdout().lock();
        serde_json::to_writer(
            &mut stdout,
            &Experiment {
                config: self,
                output,
            },
        )
        .unwrap();
        stdout.write_all(b"\n").unwrap();
    }
}

fn unlink(prefix: &str) -> io::Result<()> {
    let prefix = prefix.trim_start_matches("/");

    for entry in std::fs::read_dir("/dev/shm")? {
        let entry = entry.unwrap();
        let path = entry.path();
        let Some(name) = path.file_name().and_then(OsStr::to_str) else {
            continue;
        };
        if name.starts_with(prefix) {
            std::fs::remove_file(path)?;
        }
    }

    Ok(())
}
