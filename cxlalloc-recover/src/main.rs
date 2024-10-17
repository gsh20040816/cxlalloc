use core::cell::Cell;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::Ordering;
use std::sync::Barrier;
use std::sync::OnceLock;

use clap::Parser;
use memento::ds::queue::Dequeue;
use memento::ds::queue::Enqueue;
use memento::ds::queue::Queue;

use memento::ploc::Checkpoint;
use memento::ploc::Handle;
use memento::pmem::{Collectable, GarbageCollection, Pool, PoolHandle, RootObj};
use memento::{Collectable, Memento};

#[derive(Parser)]
struct Cli {
    /// Crash the process at this iteration
    #[arg(long)]
    process: Option<u64>,

    /// Crash a single thread at these iterations
    #[arg(long, use_value_delimiter = true, value_delimiter = ',')]
    thread: Vec<u64>,

    /// File name
    #[arg(long, default_value = "/dev/shm/pool")]
    path: String,

    /// Create or reopen
    #[arg(long)]
    create: bool,

    /// Heap size
    #[arg(long, default_value_t = 1 << 32)]
    size: usize,
}

fn main() {
    let cli = Cli::parse();

    if let Some(process) = cli.process {
        CRASH_PROCESS.store(process, Ordering::Relaxed);
    } else if !cli.thread.is_empty() {
        CRASH_THREAD.get_or_init(|| cli.thread.clone());
    }

    let pool = if cli.create {
        Pool::create::<Queue<u64>, Mmt>(&cli.path, cli.size, THREADS as usize).unwrap()
    } else {
        unsafe { Pool::open::<Queue<u64>, Mmt>(&cli.path, cli.size).unwrap() }
    };

    pool.execute::<Queue<u64>, Mmt>();
    assert_eq!(GLOBAL.load(Ordering::Relaxed), sum(OBJECTS) * THREADS);
}

#[derive(Memento, Default, Collectable)]
struct Mmt {
    i: Checkpoint<u64>,
    enq: Enqueue<u64>,
    deq: Dequeue<u64>,
}

static CRASH_PROCESS: AtomicU64 = AtomicU64::new(0);
static CRASH_THREAD: OnceLock<Vec<u64>> = OnceLock::new();

thread_local! {
    static LOCAL: Cell<u64> = const { Cell::new(0) };
}
static GLOBAL: AtomicU64 = AtomicU64::new(0);
static FINAL: u64 = sum(OBJECTS) * THREADS;
static BARRIER: Barrier = Barrier::new(THREADS as usize);

const OBJECTS: u64 = 1000;
const THREADS: u64 = 8;

const fn sum(i: u64) -> u64 {
    let mut j = 0;
    let mut sum = 0;
    while j < i {
        sum += j;
        j += 1;
    }
    sum
}

impl RootObj<Mmt> for Queue<u64> {
    fn run(&self, mmt: &mut Mmt, handle: &Handle) {
        let mut i = 0;

        let crash_process = CRASH_PROCESS.load(Ordering::Relaxed);
        let crash_thread = CRASH_THREAD.get_or_init(Vec::new);

        while i < OBJECTS {
            self.enqueue(i, &mut mmt.enq, handle);
            i = mmt.i.checkpoint(|| i + 1, handle);

            if i == crash_process {
                BARRIER.wait();
                std::process::abort();
            } else if crash_thread.contains(&i) {
                panic!()
            }
        }

        loop {
            match self.dequeue(&mut mmt.deq, handle) {
                None if LOCAL.get() == 0 && GLOBAL.load(Ordering::Acquire) == FINAL => break,
                None => {
                    let local = LOCAL.get();
                    GLOBAL.fetch_add(local, Ordering::AcqRel);
                    LOCAL.set(0);
                    std::hint::spin_loop();
                }
                Some(i) => LOCAL.set(LOCAL.get() + i),
            }
        }
    }
}
