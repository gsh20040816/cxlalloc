use core::alloc::Layout;
use core::sync::atomic::Ordering;

use crossbeam_channel::Receiver;
use crossbeam_channel::Sender;
use memento::ds::clevel::Clevel;
use memento::ds::clevel::Delete;
use memento::ds::clevel::Insert;
use memento::ds::clevel::Resize;
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
use rand::SeedableRng;

use crate::BARRIER;
use crate::BLOCK;
use crate::CACHE_COUNT;
use crate::CACHE_SIZE;
use crate::CORES;
use crate::CRASH;
use crate::CRASH_COUNT;
use crate::CRASH_DETECT;
use crate::CRASH_VICTIM;
use crate::OBJECT_COUNT;
use crate::SEED;
use crate::STOP;

// Sketchy, but taken directly from memento test harness
// https://github.com/kaist-cp/memento/blob/b88835a7c2e62d2d7f4057ca119f584cc39a1d22/src/ds/clevel.rs#L2010-L2011
pub static mut SEND: Option<[Option<Sender<()>>; 64]> = None;
pub static mut RECV: Option<Receiver<()>> = None;

#[derive(Default, Collectable, Memento)]
pub struct Mmt {
    resize: Resize<u64, PPtr<u64>>,

    i: Checkpoint<(u64, PPtr<u64>)>,
    insert: Insert<u64, PPtr<u64>>,
    delete: Delete<u64, PPtr<u64>>,
}

impl RootObj<Mmt> for Clevel<u64, PPtr<u64>> {
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

        let tid = handle.tid;

        match tid {
            // T1: Resize loop
            1 => {
                // https://github.com/kaist-cp/memento/blob/b88835a7c2e62d2d7f4057ca119f584cc39a1d22/src/ds/clevel.rs#L2070-L2073
                #[expect(static_mut_refs)]
                let recv = unsafe { RECV.as_ref().unwrap() };
                self.resize(recv, &mut mmt.resize, handle);
            }
            _ => {
                let mut i = 0;
                // https://github.com/kaist-cp/memento/blob/b88835a7c2e62d2d7f4057ca119f584cc39a1d22/src/ds/clevel.rs#L2079-L2080
                #[expect(static_mut_refs)]
                let send = unsafe { SEND.as_ref().unwrap()[tid].as_ref().unwrap() };

                if recover && block {
                    STOP.store(true, Ordering::Release);
                    BARRIER.get().unwrap().wait();
                    unsafe {
                        PMEMAllocator::gc();
                    }
                    STOP.store(false, Ordering::Release);
                    BARRIER.get().unwrap().wait();
                }

                let mut value;

                while i < object_count {
                    (i, value) = mmt.i.checkpoint(
                        || {
                            (i + 1, {
                                unsafe {
                                    handle.pool.alloc_layout::<u64>(
                                        Layout::from_size_align(
                                            rng.sample(distribution) as usize,
                                            8,
                                        )
                                        .unwrap(),
                                    )
                                }
                            })
                        },
                        handle,
                    );

                    let key = (tid as u64) << 32 | i;

                    assert!(self
                        .insert(key, value, send, &mut mmt.insert, handle)
                        .is_ok());
                    let actual = self.search(&key, handle);
                    assert_eq!(
                        actual,
                        Some(&value),
                        "expected {value:#x?}, found {actual:#x?}",
                    );

                    if tid != crash_victim {
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
                        handle.guard.flush();
                        match BLOCK.load(Ordering::Relaxed) {
                            false => {
                                CACHE_COUNT.fetch_add(
                                    unsafe { PMEMAllocator::cache_count() },
                                    Ordering::AcqRel,
                                );
                                CACHE_SIZE.fetch_add(
                                    unsafe { PMEMAllocator::cache_size() },
                                    Ordering::AcqRel,
                                );
                                panic!();
                            }
                            true => panic!(),
                        }
                    }
                }

                for i in 1..=object_count {
                    let key = (tid as u64) << 32 | i;
                    let value = *self.search(&key, handle).unwrap();
                    assert!(self.delete(&key, &mut mmt.delete, handle));
                    handle.pool.free(value);

                    if tid != crash_victim {
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

                unsafe {
                    // https://github.com/kaist-cp/memento/blob/b88835a7c2e62d2d7f4057ca119f584cc39a1d22/src/ds/clevel.rs#L2161-L2164
                    #[expect(static_mut_refs)]
                    SEND.as_mut().unwrap()[tid].take();
                }
            }
        }
    }
}
