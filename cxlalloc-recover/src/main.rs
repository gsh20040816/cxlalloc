use core::sync::atomic::Ordering;
use std::sync::Barrier;

use clap::Parser;
use cxlalloc_recover::clevel;
use cxlalloc_recover::queue;
use cxlalloc_recover::queue::Mmt;
use cxlalloc_recover::BARRIER;
use cxlalloc_recover::BLOCK;
use cxlalloc_recover::COUNT_OBJECTS;
use cxlalloc_recover::COUNT_THREADS;
use cxlalloc_recover::CRASH;
use cxlalloc_recover::CRASH_THREAD;
use cxlalloc_recover::FINAL;
use cxlalloc_recover::GLOBAL;
use memento::ds::clevel::Clevel;
use memento::ds::queue::Queue;

use memento::pmem::PPtr;
use memento::pmem::Pool;

#[derive(Parser)]
struct Cli {
    /// Crash this thread
    #[arg(long)]
    thread: Option<usize>,

    /// Crash at these iterations
    #[arg(long, use_value_delimiter = true, value_delimiter = ',')]
    crash: Vec<u64>,

    /// Block for garbage collection
    #[arg(long)]
    block: bool,

    /// File name
    #[arg(long, default_value = "/dev/shm/pool")]
    path: String,

    #[arg(long, default_value_t = 1_000_000)]
    objects: u64,

    #[arg(long)]
    threads: Option<u64>,

    /// Heap size
    #[arg(long, default_value_t = 1 << 32)]
    size: usize,
}

fn main() {
    let mut cli = Cli::parse();

    if let Some(thread) = cli.thread {
        CRASH_THREAD.store(thread, Ordering::Relaxed);
    }
    if !cli.crash.is_empty() {
        cli.crash.sort_unstable();
        cli.crash.reverse();
        *CRASH.lock().unwrap() = cli.crash.clone();
    }

    let threads = cli.threads.unwrap_or_else(|| {
        std::thread::available_parallelism()
            .expect("Call to available_parallelism failed, pass thread count explicitly")
            .get() as u64
    });

    COUNT_THREADS.store(threads, Ordering::Relaxed);
    COUNT_OBJECTS.store(cli.objects, Ordering::Relaxed);
    BLOCK.store(cli.block, Ordering::Relaxed);
    FINAL.store(queue::sum(cli.objects) * threads, Ordering::Relaxed);
    BARRIER.get_or_init(|| Barrier::new(threads as usize));

    // let pool = Pool::create::<Queue<PPtr<u64>>, queue::Mmt>(&cli.path, cli.size, threads as usize)
    //     .unwrap();
    //
    // pool.execute::<Queue<PPtr<u64>>, Mmt>();
    // assert_eq!(
    //     GLOBAL.load(Ordering::Relaxed),
    //     FINAL.load(Ordering::Relaxed),
    // );

    let (send, recv) = crossbeam_channel::bounded(8);
    unsafe {
        clevel::SEND = Some(core::array::from_fn(|_| None));
        clevel::SEND.as_mut().unwrap()[2] = Some(send);
        clevel::RECV = Some(recv);
    }

    std::thread::scope(|scope| {
        std::thread::Builder::new()
            // .stack_size(
            //     std::env::var("RUST_MIN_STACK")
            //         .ok()
            //         .and_then(|size| size.parse::<usize>().ok())
            //         .unwrap(),
            // )
            .spawn_scoped(scope, || {
                let pool = Pool::create::<Clevel<u64, u64>, clevel::Mmt>(
                    &cli.path,
                    cli.size,
                    threads as usize,
                )
                .unwrap();

                pool.execute::<Clevel<u64, u64>, clevel::Mmt>();
            })
            .unwrap();
    });
}
