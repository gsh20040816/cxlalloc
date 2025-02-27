use core::ptr::NonNull;

use crate::raw;
use crate::thread;

pub(crate) use ::crash::define;

#[test]
fn coverage() {
    crash::assert_coverage();
}

fn allocate(crash: crash::Dynamic, reclaim: bool) {
    let raw = raw::Builder::default()
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
        allocator.set_root_shared(size);
    });

    let mut allocator = raw.allocator::<usize, ()>(id);

    match allocator.root_shared() {
        None if reclaim => (),
        None => panic!("Expected allocation to be present"),

        Some(_) if reclaim => panic!("Expected allocation to be reclaimed"),
        Some(root) => {
            assert_eq!(*root, SIZE);
            unsafe { allocator.free_untyped(NonNull::from(root).cast()) };
        }
    }
}

mod unsized_to_sized {
    #[test]
    fn pre_log() {
        super::allocate(::crash::reference!(unsized_to_sized_pre_log), true);
    }
}
