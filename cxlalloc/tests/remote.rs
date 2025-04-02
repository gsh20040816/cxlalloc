use core::iter;
use core::ptr::NonNull;
use std::sync::Barrier;

use cxlalloc::raw;

#[test]
fn remote() {
    let _ = env_logger::try_init();
    let raw = raw::Raw::builder().size_small(1 << 30).build("").unwrap();

    let id = unsafe { cxlalloc::thread::Id::new(0) };
    let mut allocator = raw.allocator::<(), ()>(id);

    const ALLOCATIONS: usize = 4096;
    const THREADS: usize = 16;

    let barrier = Barrier::new(THREADS);
    let allocations = iter::from_fn(|| NonNull::new(allocator.allocate_untyped(8)))
        .map(AssertSync)
        .take(ALLOCATIONS)
        .collect::<Vec<_>>();

    std::thread::scope(|scope| {
        (1..)
            .take(THREADS)
            .map(|id| unsafe { cxlalloc::thread::Id::new(id) })
            .for_each(|id| {
                let allocations = &allocations;
                let raw = &raw;
                let barrier = &barrier;
                scope.spawn(move || {
                    let mut allocator = raw.allocator::<(), ()>(id);
                    barrier.wait();
                    for i in (0..ALLOCATIONS / THREADS).step_by(THREADS) {
                        let j = i + (u16::from(id) as usize);
                        unsafe {
                            allocator.free_untyped(allocations[j].0);
                        }
                    }
                });
            })
    });
}

struct AssertSync<T>(T);

unsafe impl<T> Sync for AssertSync<T> {}
