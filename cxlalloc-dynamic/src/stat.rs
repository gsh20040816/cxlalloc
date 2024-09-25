use core::cell::Cell;
use std::thread::LocalKey;

macro_rules! stat {
    ($($name:ident),* $(,)?) => {
        thread_local! {
            $(
                pub(crate) static $name: Cell<usize> = const { Cell::new(0) };
            )*
        }

        pub fn dump_counters(id: usize) {
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
    CALLOC,
    FREE,
    MALLOC,
    MALLOC_USABLE_SIZE,
    MEMALIGN,
    POSIX_MEMALIGN,
    REALLOC,
];

#[inline]
pub(crate) fn dec(counter: &'static LocalKey<Cell<usize>>) {
    if cfg!(feature = "stat") {
        counter.set(counter.get() - 1)
    }
}

#[inline]
pub(crate) fn inc(counter: &'static LocalKey<Cell<usize>>) {
    if cfg!(feature = "stat") {
        counter.set(counter.get() + 1)
    }
}
