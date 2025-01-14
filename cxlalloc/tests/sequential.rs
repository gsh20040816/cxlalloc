use core::ptr::NonNull;

use proptest::prelude::*;

use cxlalloc::raw;
use cxlalloc::Allocator;

const PAGE: usize = 4096;

fn with_allocator<F: FnOnce(&mut Allocator)>(apply: F) {
    let _ = env_logger::try_init();
    let raw = raw::Builder::default().build("").unwrap();
    let id = unsafe { cxlalloc::thread::Id::new(0) };
    let mut allocator = raw.allocator(id);
    apply(&mut allocator)
}

#[test]
fn create() {
    with_allocator(|_| ())
}

#[test]
fn small() {
    with_allocator(|allocator| unsafe {
        let small = allocator
            .allocate_untyped(8)
            .cast::<u64>()
            .as_mut()
            .unwrap();

        *small = 5;
        assert_eq!(*small, 5);

        allocator.free_untyped(NonNull::from(small).cast());
    })
}

#[test]
fn huge() {
    with_allocator(|allocator| unsafe {
        const SIZE: usize = 1 << 30;

        let huge = allocator
            .allocate_untyped(SIZE)
            .cast::<[u8; SIZE]>()
            .as_mut()
            .unwrap();

        for i in 0..SIZE / PAGE {
            huge[i * PAGE] = i as u8;
        }

        allocator.free_untyped(NonNull::from(huge).cast());
    })
}

proptest! {
    #[test]
    fn single(size in 1usize..(1 << 20usize)) {
        with_allocator(|allocator| unsafe {
            let allocation = allocator.allocate_untyped(size);
            allocator.free_untyped(NonNull::new(allocation).unwrap());
        })
    }
}
