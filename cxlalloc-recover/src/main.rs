use core::cell::Cell;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::AtomicUsize;
use core::sync::atomic::Ordering;
use std::sync::Barrier;

use memento::ds::queue::Dequeue;
use memento::ds::queue::Enqueue;
use memento::ds::queue::Queue;

use memento::ploc::Checkpoint;
use memento::ploc::Handle;
use memento::pmem::PAllocator;
use memento::pmem::PMEMAllocator;
use memento::pmem::{Collectable, GarbageCollection, Pool, PoolHandle, RootObj};
use memento::{Collectable, Memento};

#[derive(Memento, Default, Collectable)]
struct Mmt {
    i: Checkpoint<u64>,
    enq: Enqueue<u64>,
    deq: Dequeue<u64>,
}

static RECOVER: AtomicUsize = AtomicUsize::new(0);
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
        while i < OBJECTS {
            self.enqueue(i, &mut mmt.enq, handle);
            i = mmt.i.checkpoint(|| i + 1, handle);
        }

        if RECOVER.load(Ordering::Relaxed) == 0 {
            println!("{}:{}", handle.tid, unsafe { PMEMAllocator::measure() });
            BARRIER.wait();
            std::process::abort();
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

fn main() {
    let pool = if std::env::args().count() > 1 {
        RECOVER.store(
            std::env::args().nth(1).unwrap().parse::<usize>().unwrap(),
            Ordering::Relaxed,
        );
        unsafe { Pool::open::<Queue<u64>, Mmt>("/dev/shm/pool", 1 << 32).unwrap() }
    } else {
        Pool::create::<Queue<u64>, Mmt>("/dev/shm/pool", 1 << 32, THREADS as usize).unwrap()
    };

    pool.execute::<Queue<u64>, Mmt>();
    assert_eq!(GLOBAL.load(Ordering::Relaxed), sum(OBJECTS) * THREADS);
}
