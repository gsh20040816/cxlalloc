use core::alloc::Layout;
use core::cell::Cell;
use core::sync::atomic::AtomicBool;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::AtomicUsize;
use core::sync::atomic::Ordering;
use std::sync::Barrier;
use std::sync::Mutex;
use std::sync::OnceLock;

use clap::Parser;
use memento::ds::queue::Dequeue;
use memento::ds::queue::Enqueue;
use memento::ds::queue::Queue;

use memento::ploc::Checkpoint;
use memento::ploc::Handle;
use memento::pmem::PAllocator as _;
use memento::pmem::PMEMAllocator;
use memento::pmem::PPtr;
use memento::pmem::{Collectable, GarbageCollection, Pool, PoolHandle, RootObj};
use memento::{Collectable, Memento};

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
    FINAL.store(sum(cli.objects) * threads, Ordering::Relaxed);
    BARRIER.get_or_init(|| Barrier::new(threads as usize));

    let pool =
        Pool::create::<Queue<PPtr<u64>>, Mmt>(&cli.path, cli.size, threads as usize).unwrap();

    pool.execute::<Queue<PPtr<u64>>, Mmt>();
    assert_eq!(
        GLOBAL.load(Ordering::Relaxed),
        FINAL.load(Ordering::Relaxed),
    );
}

#[derive(Memento, Default, Collectable)]
struct Mmt {
    i: Checkpoint<u64>,
    enq: Enqueue<PPtr<u64>>,
    deq: Dequeue<PPtr<u64>>,
}

const SEED: u64 = 0xdeadbeef;
static CRASH_THREAD: AtomicUsize = AtomicUsize::new(0);
static CRASH: Mutex<Vec<u64>> = Mutex::new(Vec::new());

static COUNT_THREADS: AtomicU64 = AtomicU64::new(0);
static COUNT_OBJECTS: AtomicU64 = AtomicU64::new(0);
static BLOCK: AtomicBool = AtomicBool::new(false);

static STOP: AtomicBool = AtomicBool::new(false);
static BARRIER: OnceLock<Barrier> = OnceLock::new();

static FINAL: AtomicU64 = AtomicU64::new(0);
static GLOBAL: AtomicU64 = AtomicU64::new(0);

thread_local! {
    static LOCAL: Cell<u64> = const { Cell::new(0) };
}

const fn sum(i: u64) -> u64 {
    let mut j = 0;
    let mut sum = 0;
    while j < i {
        sum += j;
        j += 1;
    }
    sum
}

impl RootObj<Mmt> for Queue<PPtr<u64>> {
    fn run(&self, mmt: &mut Mmt, handle: &Handle) {
        core_affinity::set_for_current(core_affinity::CoreId {
            id: handle.tid + 39,
        });

        let mut rng = fastrand::Rng::with_seed(SEED.wrapping_mul(handle.tid as u64));
        let block = BLOCK.load(Ordering::Relaxed);
        let crash_thread = CRASH_THREAD.load(Ordering::Relaxed);
        let (recover, crash) = if handle.tid == crash_thread {
            let poisoned = CRASH.is_poisoned();
            CRASH.clear_poison();
            (poisoned, &mut *CRASH.lock().unwrap())
        } else {
            (false, &mut Vec::new())
        };
        let objects = COUNT_OBJECTS.load(Ordering::Relaxed);

        let mut i = 0;

        if recover && block {
            STOP.store(true, Ordering::Release);
            BARRIER.get().unwrap().wait();
            unsafe {
                PMEMAllocator::gc();
            }
            STOP.store(false, Ordering::Release);
            BARRIER.get().unwrap().wait();
        }

        while i < objects {
            unsafe {
                let pointer = handle.pool.alloc_layout::<u64>(
                    Layout::from_size_align(rng.u16(8..1024) as usize, 8).unwrap(),
                );

                *pointer.deref_mut(handle.pool) = i;
                self.enqueue(pointer, &mut mmt.enq, handle);
                i = mmt.i.checkpoint(|| i + 1, handle);
            };

            // Check for GC request
            if handle.tid != crash_thread {
                if !STOP.load(Ordering::Relaxed) {
                    continue;
                }
                BARRIER.get().unwrap().wait();
                BARRIER.get().unwrap().wait();
                unsafe {
                    PMEMAllocator::invalidate();
                }
            }

            if crash.last().copied() == Some(i) {
                crash.pop();
                match BLOCK.load(Ordering::Relaxed) {
                    false => {
                        println!("LEAK:{}", unsafe { PMEMAllocator::measure() });
                        panic!();
                    }
                    true => {
                        panic!();
                    }
                }
            }
        }

        let r#final = FINAL.load(Ordering::Relaxed);

        loop {
            match self.dequeue(&mut mmt.deq, handle) {
                None if LOCAL.get() == 0 => {
                    if GLOBAL.load(Ordering::Relaxed) == r#final {
                        break;
                    } else {
                        std::hint::spin_loop();
                    }
                }
                None => {
                    let local = LOCAL.get();
                    GLOBAL.fetch_add(local, Ordering::AcqRel);
                    LOCAL.set(0);
                    std::hint::spin_loop();
                }
                Some(pointer) => {
                    let i = unsafe { *pointer.deref(handle.pool) };
                    LOCAL.set(LOCAL.get() + i);
                    handle.pool.free(pointer);
                }
            }

            if handle.tid != crash_thread {
                if !STOP.load(Ordering::Relaxed) {
                    continue;
                }
                BARRIER.get().unwrap().wait();
                BARRIER.get().unwrap().wait();
                unsafe {
                    PMEMAllocator::invalidate();
                }
            }
        }
    }
}
