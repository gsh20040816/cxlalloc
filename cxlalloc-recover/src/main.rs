use core::sync::atomic::Ordering;
use std::io;
use std::sync::Barrier;
use std::time::Instant;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use clap::Parser;
use clap::ValueEnum;
use cxlalloc_recover::clevel;
use cxlalloc_recover::queue;
use cxlalloc_recover::BARRIER;
use cxlalloc_recover::BLOCK;
use cxlalloc_recover::CACHE_COUNT;
use cxlalloc_recover::CACHE_SIZE;
use cxlalloc_recover::CRASH;
use cxlalloc_recover::CRASH_COUNT;
use cxlalloc_recover::CRASH_VICTIM;
use cxlalloc_recover::FINAL;
use cxlalloc_recover::GLOBAL;
use cxlalloc_recover::OBJECT_COUNT;
use cxlalloc_recover::THREAD_COUNT;
use memento::ds::clevel::Clevel;

use memento::ds::queue::Queue;
use memento::pmem::PAllocator as _;
use memento::pmem::PMEMAllocator;
use memento::pmem::PPtr;
use memento::pmem::Pool;
use serde::Serialize;

#[derive(Parser, Serialize)]
struct Config {
    #[arg(skip)]
    allocator: Allocator,

    /// Crash this thread
    #[arg(long)]
    crash_victim: Option<usize>,

    #[arg(long)]
    crash_count: u64,

    /// Block for garbage collection
    #[arg(long)]
    block: bool,

    /// File name
    #[serde(skip)]
    #[arg(long, default_value = "/dev/shm/pool")]
    path: String,

    #[arg(long, default_value_t = 1_000_000)]
    object_count: u64,

    #[arg(long)]
    thread_count: u64,

    /// Heap size
    #[arg(long, default_value_t = 1 << 32)]
    heap_size: usize,

    #[arg(long)]
    workload: Workload,
}

#[derive(Clone, ValueEnum, Serialize)]
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

#[derive(Clone, ValueEnum, Serialize)]
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

fn main() {
    let config = Config::parse();

    if let Some(thread) = config.crash_victim {
        CRASH_VICTIM.store(thread, Ordering::Relaxed);
    }

    THREAD_COUNT.store(config.thread_count, Ordering::Relaxed);
    OBJECT_COUNT.store(config.object_count, Ordering::Relaxed);
    CRASH_COUNT.store(config.crash_count, Ordering::Relaxed);
    CRASH.store(
        match config.crash_count {
            0 => u64::MAX,
            crash_count => config.object_count / (crash_count + 1),
        },
        Ordering::Relaxed,
    );

    BLOCK.store(config.block, Ordering::Relaxed);

    let time;

    match config.workload {
        Workload::Queue => {
            FINAL.store(
                queue::sum(config.object_count) * config.thread_count,
                Ordering::Relaxed,
            );
            BARRIER.get_or_init(|| Barrier::new(config.thread_count as usize));

            let pool = Pool::create::<Queue<PPtr<u64>>, queue::Mmt>(
                &config.path,
                config.heap_size,
                config.thread_count as usize,
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
            BARRIER.get_or_init(|| Barrier::new(config.thread_count as usize - 1));
            let (send, recv) = crossbeam_channel::bounded(8);
            unsafe {
                clevel::SEND = Some(core::array::from_fn(|_| None));
                for i in (2..).take(config.thread_count as usize - 1) {
                    clevel::SEND.as_mut().unwrap()[i] = Some(send.clone());
                }
                clevel::RECV = Some(recv);
                drop(send);
            }

            let pool = Pool::create::<Clevel<u64, PPtr<u64>>, clevel::Mmt>(
                &config.path,
                config.heap_size,
                config.thread_count as usize,
            )
            .unwrap();

            let start = Instant::now();
            pool.execute::<Clevel<u64, PPtr<u64>>, clevel::Mmt>();
            time = start.elapsed();
        }
    }

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
    serde_json::to_writer(&mut stdout, &Experiment { config, output }).unwrap();
}
