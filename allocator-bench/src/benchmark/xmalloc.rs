// https://github.com/emeryberger/Hoard/blob/f021bdb810332c9c9f5a11ae5404aaa38fe129c0/benchmarks/threadtest/threadtest.cpp

use core::cmp;
use core::mem;
use core::mem::MaybeUninit;
use core::num::NonZeroU64;
use core::ptr::NonNull;
use core::ptr::addr_of_mut;
use core::sync::atomic::AtomicBool;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::Ordering;
use core::time::Duration;

use bon::Builder;
use rand::RngCore as _;
use rand::SeedableRng as _;
use rand::rngs::SmallRng;
use serde::Deserialize;
use serde::Serialize;
use shm::Shm;

use crate::Allocator;
use crate::allocator;
use crate::allocator::Backend;
use crate::allocator::Handle;
use crate::benchmark;
use crate::config;

#[derive(Builder, Clone, Debug, Deserialize, Serialize)]
pub struct Xmalloc {
    #[builder(default = 100)]
    limit: u64,

    #[builder(default = 5)]
    time: u64,
}

const OBJECTS_PER_BATCH: usize = 120;
const POSSIBLE_SIZES: &[usize] = &[
    8,
    12,
    16,
    24,
    32,
    48,
    64,
    96,
    128,
    192,
    256,
    (256 * 3) / 2,
    512,
    (512 * 3) / 2,
    // 1024,
    // (1024 * 3) / 2,
    // 2048,
];

struct Batch {
    next: u64,
    objects: [u64; OBJECTS_PER_BATCH],
}

#[repr(C)]
struct Root {
    lock: libc::pthread_mutex_t,
    empty: libc::pthread_cond_t,
    full: libc::pthread_cond_t,

    len: AtomicU64,
    head: AtomicU64,
}

pub struct Global {
    root: Shm<Root>,
    stop: AtomicBool,
}

#[derive(Serialize)]
pub struct OutputWorker {
    operations: u64,
}

#[derive(Serialize)]
pub struct Output {
    throughput: u64,
}

unsafe impl Sync for Global {}

impl<B: Backend> benchmark::Benchmark<B> for Xmalloc {
    const NAME: &str = "xm";
    type StateGlobal = Global;
    type StateCoordinator = ();
    type StateWorker = SmallRng;

    type OutputWorker = OutputWorker;
    type OutputCoordinator = u64;
    type OutputGlobal = Output;

    fn setup_process(
        &self,
        config: &config::Process,
        allocator: &allocator::Config,
    ) -> Self::StateGlobal {
        let root = Shm::<Root>::new(Some(allocator.numa), c"xmalloc".to_owned(), false).unwrap();
        let stop = AtomicBool::new(false);

        if config.process_id == 0 {
            unsafe {
                let mut attr = MaybeUninit::<libc::pthread_mutexattr_t>::zeroed();
                libc::pthread_mutexattr_init(attr.as_mut_ptr());
                libc::pthread_mutexattr_setpshared(attr.as_mut_ptr(), libc::PTHREAD_PROCESS_SHARED);

                let lock = core::ptr::addr_of_mut!((*root.address_mut()).lock);
                libc::pthread_mutex_init(lock, attr.as_ptr());

                let mut attr = MaybeUninit::<libc::pthread_condattr_t>::zeroed();
                libc::pthread_condattr_init(attr.as_mut_ptr());
                libc::pthread_condattr_setpshared(attr.as_mut_ptr(), libc::PTHREAD_PROCESS_SHARED);

                let empty = core::ptr::addr_of_mut!((*root.address_mut()).empty);
                libc::pthread_cond_init(empty, attr.as_ptr());

                let full = core::ptr::addr_of_mut!((*root.address_mut()).full);
                libc::pthread_cond_init(full, attr.as_ptr());
            }
        }

        Global { root, stop }
    }

    fn setup_coordinator(
        &self,
        _config: &config::Process,
        _global: &Self::StateGlobal,
    ) -> Self::StateCoordinator {
    }

    fn setup_worker(
        &self,
        config: &config::Thread,
        _global: &Self::StateGlobal,
        _allocator: &mut B::Allocator,
    ) -> Self::StateWorker {
        SmallRng::seed_from_u64(config.thread_id as u64)
    }

    fn run_coordinator(
        &self,
        _: &config::Process,
        global: &Self::StateGlobal,
        (): &mut Self::StateCoordinator,
    ) -> Self::OutputCoordinator {
        std::thread::sleep(Duration::from_secs(self.time));
        global.stop.store(true, Ordering::Relaxed);

        unsafe {
            let root = global.root.address_mut();
            libc::pthread_cond_broadcast(addr_of_mut!((*root).empty));
            libc::pthread_cond_broadcast(addr_of_mut!((*root).full));
        }

        self.time
    }

    fn run_worker(
        &self,
        config: &config::Thread,
        global: &Self::StateGlobal,
        rng: &mut Self::StateWorker,
        allocator: &mut B::Allocator,
    ) -> Self::OutputWorker {
        // Allocator
        let mut operations = 0;

        if config.thread_id & 1 == 0 {
            while !global.stop.load(Ordering::Relaxed) {
                let batch = allocator.allocate(mem::size_of::<Batch>()).unwrap();

                for i in 0..OBJECTS_PER_BATCH {
                    let size = POSSIBLE_SIZES[rng.next_u32() as usize % POSSIBLE_SIZES.len()];
                    let object = allocator.allocate(size).unwrap();

                    unsafe {
                        libc::memset(object.as_ptr(), i as u8 as i32, cmp::min(128, size));
                    }

                    unsafe {
                        allocator.link(
                            addr_of_mut!((*batch.as_ptr().cast::<Batch>()).objects[i]),
                            &object,
                        );
                    }
                }

                operations += OBJECTS_PER_BATCH + 1;
                global.push(self, allocator, batch);
            }
        // Releaser
        } else {
            while !global.stop.load(Ordering::Relaxed) {
                let Some(handle) = global.pop(allocator) else {
                    continue;
                };

                let batch = unsafe { handle.as_ptr().cast::<Batch>().as_mut().unwrap() };

                for offset in &mut batch.objects {
                    unsafe {
                        allocator.unlink(offset);
                    }
                }

                unsafe {
                    allocator.deallocate(handle);
                }

                operations += OBJECTS_PER_BATCH + 1;
            }
        }

        OutputWorker {
            operations: operations as u64,
        }
    }

    fn teardown_process(&self, config: &config::Process, mut global: Self::StateGlobal) {
        if config.process_id != 0 {
            return;
        }

        global.root.unlink().unwrap();
    }

    fn aggregate(
        time: Self::OutputCoordinator,
        workers: Vec<Self::OutputWorker>,
    ) -> Self::OutputGlobal {
        let operations = workers
            .iter()
            .map(|OutputWorker { operations }| *operations)
            .sum::<u64>();

        Output {
            throughput: operations / time,
        }
    }
}

impl Global {
    fn push<A: Allocator>(&self, config: &Xmalloc, allocator: &mut A, handle: A::Handle) {
        let batch = unsafe {
            handle
                .as_ptr()
                .cast::<MaybeUninit<Batch>>()
                .as_mut()
                .unwrap()
        };

        let root = unsafe { &*self.root.address() };

        unsafe {
            libc::pthread_mutex_lock(&root.lock as *const _ as *mut _);
        }

        while root.len.load(Ordering::Relaxed) >= config.limit && !self.stop.load(Ordering::Relaxed)
        {
            unsafe {
                libc::pthread_cond_wait(
                    &root.full as *const _ as *mut _,
                    &root.lock as *const _ as *mut _,
                );
            }
        }

        let next =
            unsafe { AtomicU64::from_ptr(core::ptr::addr_of_mut!((*batch.as_mut_ptr()).next)) };

        next.store(root.head.load(Ordering::Relaxed), Ordering::Relaxed);

        unsafe {
            allocator.link(root.head.as_ptr(), &handle);
        }

        root.len
            .store(root.len.load(Ordering::Relaxed) + 1, Ordering::Relaxed);

        unsafe {
            libc::pthread_cond_signal(&root.empty as *const _ as *mut _);
            libc::pthread_mutex_unlock(&root.lock as *const _ as *mut _);
        }
    }

    fn pop<A: Allocator>(&self, allocator: &mut A) -> Option<A::Handle> {
        let root = unsafe { &*self.root.address() };

        unsafe {
            libc::pthread_mutex_lock(&root.lock as *const _ as *mut _);
        }

        while root.head.load(Ordering::Relaxed) == 0 && !self.stop.load(Ordering::Relaxed) {
            unsafe {
                libc::pthread_cond_wait(
                    &root.empty as *const _ as *mut _,
                    &root.lock as *const _ as *mut _,
                );
            }
        }

        let Some(head) = NonZeroU64::new(root.head.load(Ordering::Relaxed)) else {
            unsafe {
                libc::pthread_mutex_unlock(&root.lock as *const _ as *mut _);
            }
            return None;
        };

        let handle = allocator.offset_to_handle(head);
        let pointer = handle.as_ptr();
        let next = unsafe { pointer.cast::<Batch>().as_ref().unwrap().next };

        // HACK: only cxl-shm needs to decrement reference count here
        if std::any::type_name::<A>().contains("cxl_shm") {
            unsafe {
                allocator.unlink(root.head.as_ptr());
            }
        }

        root.head.store(next, Ordering::Relaxed);
        root.len
            .store(root.len.load(Ordering::Relaxed) - 1, Ordering::Relaxed);

        unsafe {
            libc::pthread_cond_signal(&root.full as *const _ as *mut _);
            libc::pthread_mutex_unlock(&root.lock as *const _ as *mut _);
        }

        Some(handle)
    }
}
