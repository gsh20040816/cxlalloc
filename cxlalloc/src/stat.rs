use core::cell::Cell;
use core::fmt::Write as _;
use std::thread::LocalKey;

use crate::size;
use crate::size::Bracket as _;

pub fn dump(id: usize) {
    dump_counters(id);
    dump_sizes(id);
}

macro_rules! stat {
    ($($name:ident),* $(,)?) => {
        thread_local! {
            $(
                pub(crate) static $name: Cell<usize> = const { Cell::new(0) };
            )*
        }

        pub fn dump_counters(id: usize) {
            if !cfg!(feature = "stat-count") {
                return;
            }

            $(
                eprintln!("{},{},{}", id, stringify!($name), $name.get());
                $name.set(0);
            )*
        }

    };
}

stat![
    ALLOCATE,
    ALLOCATE_FAST,
    ALLOCATE_FAST_DETACH,
    ALLOCATE_FAST_DISOWN,
    ALLOCATE_LARGE,
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
    FREE_REMOTE,
    FREE_REMOTE_GLOBAL,
    FREE_REMOTE_GLOBAL_WIN,
    FREE_REMOTE_GLOBAL_WIN_STEAL,
    FREE_REMOTE_GLOBAL_LOSE,
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
    FLUSH,
    FENCE,
    REMOTE,
    REMOTE_CONTEND,
];

#[inline]
pub(crate) fn inc(counter: &'static LocalKey<Cell<usize>>) {
    if !cfg!(feature = "stat-count") {
        return;
    }

    counter.set(counter.get() + 1)
}

thread_local! {
    static SMALL: size::Array<size::Small, Cell<usize>> = size::Array::default();
    static LARGE: size::Array<size::Large, Cell<usize>> = size::Array::default();

    // FIXME: this leaks memory, but we need it for now if we want to track
    // size statistics through the `LD_PRELOAD` shim.
    //
    // The problem is that `Histogram` implements `Drop` implicitly due
    // to its inner allocations. While we don't have the same problem with
    // recursive initialization, since this thread local is not accessed
    // during allocator initialization, its destructor is called before
    // the cxlalloc-dynamic main thread destructor, which tries to dump
    // statistics and panics.
    #[cfg(feature = "stat-size")]
    static HUGE: core::mem::ManuallyDrop<core::cell::RefCell<hdrhistogram::Histogram<u64>>>  = core::mem::ManuallyDrop::new(core::cell::RefCell::new(
        hdrhistogram::Histogram::new(3).unwrap()
    ));
}

#[inline]
pub(crate) fn record_small(class: size::Small) {
    if !cfg!(feature = "stat-size") {
        return;
    }

    SMALL.with(|counters| counters[class].set(counters[class].get() + 1))
}

#[inline]
pub(crate) fn record_large(class: size::Large) {
    if !cfg!(feature = "stat-size") {
        return;
    }

    LARGE.with(|counters| counters[class].set(counters[class].get() + 1))
}

#[inline]
pub(crate) fn record_huge(_huge: usize) {
    #[cfg(feature = "stat-size")]
    HUGE.with(|histogram| histogram.borrow_mut().record(_huge as u64).unwrap())
}

pub fn dump_sizes(id: usize) {
    if !cfg!(feature = "stat-size") {
        return;
    }

    let mut output = format!("{id}");

    SMALL.with(|counters| {
        for (class, count) in counters.iter().filter(|(_, count)| count.get() > 0) {
            write!(&mut output, ",{}:{}", class.size(), count.get()).unwrap();
        }
    });

    LARGE.with(|counters| {
        for (class, count) in counters.iter().filter(|(_, count)| count.get() > 0) {
            write!(&mut output, ",{}:{}", class.size(), count.get()).unwrap();
        }
    });

    #[cfg(feature = "stat-size")]
    HUGE.with(|histogram| {
        for value in histogram.borrow().iter_recorded() {
            write!(
                &mut output,
                ",{}:{}",
                value.value_iterated_to(),
                value.count_at_value()
            )
            .unwrap();
        }
    });

    eprintln!("{output}");
}
