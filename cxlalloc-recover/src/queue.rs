use core::alloc::Layout;
use core::sync::atomic::Ordering;

use memento::ds::queue::Dequeue;
use memento::ds::queue::Enqueue;
use memento::ds::queue::Queue;

use memento::ploc::Checkpoint;
use memento::ploc::Handle;
use memento::pmem::Collectable;
use memento::pmem::GarbageCollection;
use memento::pmem::PAllocator as _;
use memento::pmem::PMEMAllocator;
use memento::pmem::PPtr;
use memento::pmem::PoolHandle;
use memento::pmem::RootObj;
use memento::Collectable;
use memento::Memento;
use rand::distr::Uniform;
use rand::rngs::SmallRng;
use rand::Rng as _;
use rand::SeedableRng as _;

use crate::BARRIER;
use crate::BLOCK;
use crate::CACHE_COUNT;
use crate::CACHE_SIZE;
use crate::CORES;
use crate::CRASH;
use crate::CRASH_COUNT;
use crate::CRASH_DETECT;
use crate::CRASH_VICTIM;
use crate::FINAL;
use crate::GLOBAL;
use crate::LOCAL;
use crate::OBJECT_COUNT;
use crate::SEED;
use crate::STOP;

#[derive(Memento, Default, Collectable)]
pub struct Mmt {
    i: Checkpoint<(u64, PPtr<u64>)>,
    enq: Enqueue<PPtr<u64>>,
    deq: Dequeue<PPtr<u64>>,
}

impl RootObj<Mmt> for Queue<PPtr<u64>> {
    fn run(&self, mmt: &mut Mmt, handle: &Handle) {
        core_affinity::set_for_current(CORES[handle.tid % CORES.len()]);

        let mut rng = SmallRng::seed_from_u64(SEED.wrapping_mul(handle.tid as u64));
        let distribution = Uniform::new(8, 1000).unwrap();

        let block = BLOCK.load(Ordering::Relaxed);
        let crash_victim = CRASH_VICTIM.load(Ordering::Relaxed);
        let object_count = OBJECT_COUNT.load(Ordering::Relaxed);
        let crash = CRASH.load(Ordering::Relaxed);
        let mut crash_count = CRASH_COUNT.load(Ordering::Relaxed);

        let (recover, detect) = if handle.tid == crash_victim {
            let poisoned = CRASH_DETECT.is_poisoned();
            CRASH_DETECT.clear_poison();
            (poisoned, &mut *CRASH_DETECT.lock().unwrap())
        } else {
            (false, &mut 0)
        };

        if recover && block {
            STOP.store(true, Ordering::Release);
            BARRIER.get().unwrap().wait();
            unsafe {
                PMEMAllocator::gc();
            }
            STOP.store(false, Ordering::Release);
            BARRIER.get().unwrap().wait();
        }

        let mut i = 0;
        let mut value;

        while i < object_count {
            unsafe {
                (i, value) = mmt.i.checkpoint(
                    || {
                        (i + 1, {
                            let pointer = handle.pool.alloc_layout::<u64>(
                                Layout::from_size_align(rng.sample(distribution), 8).unwrap(),
                            );

                            *pointer.deref_mut(handle.pool) = i;
                            pointer
                        })
                    },
                    handle,
                );
                self.enqueue(value, &mut mmt.enq, handle);
            };

            // Check for GC request
            if handle.tid != crash_victim {
                if !STOP.load(Ordering::Relaxed) {
                    continue;
                }
                BARRIER.get().unwrap().wait();
                BARRIER.get().unwrap().wait();
                crash_count -= 1;
                unsafe {
                    PMEMAllocator::invalidate();
                }
            } else if i % crash == 0 && i > *detect && *detect / crash < crash_count {
                *detect = i;
                match BLOCK.load(Ordering::Relaxed) {
                    false => {
                        CACHE_COUNT
                            .fetch_add(unsafe { PMEMAllocator::cache_count() }, Ordering::AcqRel);
                        CACHE_SIZE
                            .fetch_add(unsafe { PMEMAllocator::cache_size() }, Ordering::AcqRel);
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

            if handle.tid != crash_victim {
                if !STOP.load(Ordering::Relaxed) {
                    continue;
                }
                BARRIER.get().unwrap().wait();
                BARRIER.get().unwrap().wait();
                crash_count -= 1;
                unsafe {
                    PMEMAllocator::invalidate();
                }
            }
        }

        if handle.tid != crash_victim {
            while block && crash_count > 0 {
                BARRIER.get().unwrap().wait();
                BARRIER.get().unwrap().wait();
                crash_count -= 1;
            }
        }
    }
}

pub const fn sum(i: u64) -> u64 {
    let mut j = 0;
    let mut sum = 0;
    while j < i {
        sum += j;
        j += 1;
    }
    sum
}
