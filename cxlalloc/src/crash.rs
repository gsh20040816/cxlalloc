use core::ptr::NonNull;
use core::sync::atomic::Ordering;

use crate::raw;
use crate::thread;

pub(crate) use ::crash::define;

#[expect(unused)]
fn allocate(crash: crash::Dynamic, reclaim: bool) {
    let raw = raw::Raw::builder()
        .size_small(2usize.pow(28))
        .build("")
        .unwrap();

    const SIZE: usize = 8;
    let id = unsafe { thread::Id::new(0) };

    ::crash::run(crash, || unsafe {
        let mut allocator = raw.allocator::<usize, ()>(id);
        let size = allocator
            .allocate_untyped(SIZE)
            .cast::<usize>()
            .as_mut()
            .unwrap();
        *size = SIZE;
        allocator.set_root_shared(size, Ordering::Release);
    });

    let mut allocator = raw.allocator::<usize, ()>(id);

    match allocator.root_shared(Ordering::Acquire) {
        None if reclaim => (),
        None => panic!("Expected allocation to be present"),

        Some(_) if reclaim => panic!("Expected allocation to be reclaimed"),
        Some(root) => {
            assert_eq!(*root, SIZE);
            unsafe { allocator.free_untyped(NonNull::from(root).cast()) };
        }
    }
}

// FIXME
//
// #[test]
// fn coverage() {
//     crash::assert_coverage();
// }
//
// mod unsized_to_sized {
//     #[test]
//     fn pre_log() {
//         super::allocate(::crash::reference!(unsized_to_sized_pre_log), true);
//     }
// }
