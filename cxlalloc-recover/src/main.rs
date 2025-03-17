use core::sync::atomic::Ordering;
use std::sync::Barrier;

use clap::Parser;
use clap::ValueEnum;
use cxlalloc_recover::clevel;
use cxlalloc_recover::queue;
use cxlalloc_recover::BARRIER;
use cxlalloc_recover::BLOCK;
use cxlalloc_recover::CRASH;
use cxlalloc_recover::CRASH_VICTIM;
use cxlalloc_recover::FINAL;
use cxlalloc_recover::GLOBAL;
use cxlalloc_recover::OBJECT_COUNT;
use cxlalloc_recover::THREAD_COUNT;
use memento::ds::clevel::Clevel;

use memento::ds::queue::Queue;
use memento::pmem::PPtr;
use memento::pmem::Pool;

#[derive(Parser)]
struct Cli {
    /// Crash this thread
    #[arg(long)]
    crash_victim: Option<usize>,

    crash_count: u64,

    /// Block for garbage collection
    #[arg(long)]
    block: bool,

    /// File name
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

#[derive(Clone, ValueEnum)]
pub enum Workload {
    Queue,
    Clevel,
}

fn main() {
    let cli = Cli::parse();

    if let Some(thread) = cli.crash_victim {
        CRASH_VICTIM.store(thread, Ordering::Relaxed);
    }

    THREAD_COUNT.store(cli.thread_count, Ordering::Relaxed);
    OBJECT_COUNT.store(cli.object_count, Ordering::Relaxed);
    CRASH.store(
        cli.object_count
            .checked_div(cli.crash_count)
            .unwrap_or(u64::MAX),
        Ordering::Relaxed,
    );

    BLOCK.store(cli.block, Ordering::Relaxed);

    match cli.workload {
        Workload::Queue => {
            FINAL.store(
                queue::sum(cli.object_count) * cli.thread_count,
                Ordering::Relaxed,
            );
            BARRIER.get_or_init(|| Barrier::new(cli.thread_count as usize));

            let pool = Pool::create::<Queue<PPtr<u64>>, queue::Mmt>(
                &cli.path,
                cli.heap_size,
                cli.thread_count as usize,
            )
            .unwrap();

            pool.execute::<Queue<PPtr<u64>>, queue::Mmt>();
            assert_eq!(
                GLOBAL.load(Ordering::Relaxed),
                FINAL.load(Ordering::Relaxed),
            );
        }
        Workload::Clevel => {
            BARRIER.get_or_init(|| Barrier::new(cli.thread_count as usize - 1));
            let (send, recv) = crossbeam_channel::bounded(8);
            unsafe {
                clevel::SEND = Some(core::array::from_fn(|_| None));
                for i in (2..).take(cli.thread_count as usize - 1) {
                    clevel::SEND.as_mut().unwrap()[i] = Some(send.clone());
                }
                clevel::RECV = Some(recv);
                drop(send);
            }

            let pool = Pool::create::<Clevel<u64, PPtr<u64>>, clevel::Mmt>(
                &cli.path,
                cli.heap_size,
                cli.thread_count as usize,
            )
            .unwrap();

            pool.execute::<Clevel<u64, PPtr<u64>>, clevel::Mmt>();
        }
    }
}
