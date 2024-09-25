use core::cell::Cell;
use core::cell::RefCell;
use core::fmt::Write as _;
use std::thread::LocalKey;

use crate::size;

macro_rules! stat {
    ($($name:ident),* $(,)?) => {
        thread_local! {
            $(
                pub(crate) static $name: Cell<usize> = const { Cell::new(0) };
            )*
        }

        pub fn dump_counters(id: usize) {
            if !cfg!(feature = "stat") {
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
    if !cfg!(feature = "stat") {
        return;
    }

    counter.set(counter.get() + 1)
}

thread_local! {
    static SMALL: size::Array<Cell<usize>> = size::Array::default();

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
    static LARGE: core::mem::ManuallyDrop<RefCell<hdrhistogram::Histogram<u64>>>  = core::mem::ManuallyDrop::new(RefCell::new(
        hdrhistogram::Histogram::new(3).unwrap()
    ));
}

#[inline]
pub(crate) fn record(class: size::Class) {
    if !cfg!(feature = "stat-size") {
        return;
    }

    match class {
        size::Class::Small(small) => {
            SMALL.with(|counters| counters[small].set(counters[small].get() + 1))
        }
        size::Class::Large(_large) =>
        {
            #[cfg(feature = "stat-size")]
            LARGE.with(|histogram| {
                histogram
                    .borrow_mut()
                    .record(_large.count() as u64)
                    .unwrap()
            })
        }
    }
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

    #[cfg(feature = "stat-size")]
    LARGE.with(|histogram| {
        for value in histogram.borrow().iter_recorded() {
            write!(
                &mut output,
                ",{}:{}",
                value.value_iterated_to() * crate::SIZE_SLAB as u64,
                value.count_at_value()
            )
            .unwrap();
        }
    });

    eprintln!("{output}");
}
