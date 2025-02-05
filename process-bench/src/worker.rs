use core::sync::atomic::AtomicU64;
use core::sync::atomic::Ordering;
use std::time::Instant;

use crate::Allocator;

pub fn run<A: Allocator>(name: &str, size: usize, id: u64, barrier: u64) {
    let mut shm = A::open(name, size);

    let barrier = unsafe { AtomicU64::from_ptr(shm.offset_to_address(barrier).cast()) };
    match barrier.fetch_sub(1, Ordering::Relaxed) {
        0 => unreachable!(),
        1 => (),
        _ => while barrier.load(Ordering::Relaxed) > 0 {},
    }

    let start = Instant::now();

    for _ in 0..100_000 {
        let pointer = shm.allocate(8);
        unsafe {
            shm.deallocate(pointer);
        }
    }

    println!("{},{}", id, start.elapsed().as_micros());
}
