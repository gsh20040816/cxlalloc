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
];

#[inline]
pub(crate) fn inc(counter: &'static LocalKey<Cell<usize>>) {
    if cfg!(feature = "stat") {
        counter.set(counter.get() + 1)
    }
}
