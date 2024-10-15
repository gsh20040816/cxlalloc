use core::ptr::NonNull;

use crate::raw;
use crate::root;
use crate::thread;

pub(crate) use ::crash::define;

#[test]
fn coverage() {
    crash::assert_coverage();
}

fn allocate(crash: crash::Dynamic, reclaim: bool) {
    let raw = raw::Builder::default()
        .size(2usize.pow(28))
        .build("")
        .unwrap();

    const SIZE: usize = 8;
    let id = unsafe { thread::Id::new(0) };
    let index = root::Index::new(0);

    ::crash::run(crash, || unsafe {
        let mut allocator = raw.allocator(id);
        let pointer = allocator.allocate_untyped(SIZE);
        pointer.cast::<u64>().write_volatile(SIZE as u64);
        allocator.set_root_untyped(index, Some(NonNull::new(pointer).unwrap()));
    });

    let mut allocator = raw.allocator(id);

    match unsafe { allocator.root_untyped(index) } {
        None if reclaim => (),
        None => panic!("Expected allocation to be present"),

        Some(_) if reclaim => panic!("Expected allocation to be reclaimed"),
        Some(pointer) => {
            assert_eq!(
                unsafe { pointer.cast::<u64>().as_ptr().read_volatile() },
                SIZE as u64,
            );

            unsafe { allocator.free_untyped(pointer) };
        }
    }
}

mod unsized_to_sized {
    #[test]
    fn pre_log() {
        super::allocate(::crash::reference!(unsized_to_sized_pre_log), true);
    }
}
