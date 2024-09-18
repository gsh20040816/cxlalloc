use core::cell::Cell;
use std::thread::LocalKey;

macro_rules! stat {
    ($($name:ident),* $(,)?) => {
        thread_local! {
            $(
                pub(crate) static $name: Cell<usize> = const { Cell::new(0) };
            )*
        }

        pub fn dump(id: usize) {
            if cfg!(feature = "stat") {
                $(
                    eprintln!("{},{},{}", id, stringify!($name), $name.get());
                    $name.set(0);
                )*
            }
        }

    };
}

stat![
    ALLOCATE,
    ALLOCATE_FAST,
    ALLOCATE_FAST_DETACH,
    ALLOCATE_LARGE,
    ALLOCATE_LARGE_UNSIZED,
    ALLOCATE_LARGE_GLOBAL,
    ALLOCATE_LARGE_BUMP,
    ALLOCATE_SMALL,
    ALLOCATE_SMALL_ZERO,
    ALLOCATE_SMALL_UNSIZED,
    ALLOCATE_SMALL_GLOBAL,
    ALLOCATE_SMALL_BUMP,
    FREE,
    FREE_FAST,
    FREE_FAST_ATTACH,
    FREE_FAST_UNSIZED,
    FREE_LARGE,
    FREE_LARGE_UNSIZED,
    FREE_LARGE_GLOBAL,
    FREE_REMOTE,
    FREE_REMOTE_GLOBAL,
    BUMP,
    BUMP_ALLOCATE,
    BUMP_ALLOCATE_CONTEND,
    BUMP_ALLOCATE_CONTEND_INTERLEAVE,
    BUMP_ALLOCATE_CONTEND_HELP,
    GLOBAL,
    GLOBAL_PUSH,
    GLOBAL_PUSH_CONTEND,
    GLOBAL_PUSH_CONTEND_INTERLEAVE,
    GLOBAL_PUSH_CONTEND_HELP,
    GLOBAL_POP,
    GLOBAL_POP_CONTEND,
    GLOBAL_POP_CONTEND_INTERLEAVE,
    GLOBAL_POP_CONTEND_HELP,
];

#[inline]
pub(crate) fn inc(counter: &'static LocalKey<Cell<usize>>) {
    if cfg!(feature = "stat") {
        counter.set(counter.get() + 1)
    }
}
