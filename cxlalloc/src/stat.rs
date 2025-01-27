use core::cell::Cell;
use core::cell::UnsafeCell;
use core::fmt::Write as _;
use core::mem;
use core::sync::atomic::AtomicI64;
use core::sync::atomic::Ordering;
use std::sync::LazyLock;
use std::thread::LocalKey;

use crate::allocator;
use crate::heap;
use crate::huge;
use crate::size;
use crate::size::Bracket as _;
use crate::slab;
use crate::thread;
use crate::Raw;

pub fn dump(id: usize) {
    dump_memory_global(id);
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

static MEMORY_GLOBAL_SHARED_LOOSE: LazyLock<usize> =
    LazyLock::new(|| Raw::shared().0.get().next_multiple_of(crate::SIZE_PAGE));

const MEMORY_GLOBAL_SHARED_TIGHT: usize = mem::size_of::<allocator::Shared<()>>()
    + mem::size_of::<heap::Shared<size::Small>>()
    + mem::size_of::<heap::Shared<size::Large>>()
    + mem::size_of::<huge::Shared>();

static MEMORY_GLOBAL_OWNED_LOOSE: LazyLock<usize> =
    LazyLock::new(|| Raw::owned().0.get().next_multiple_of(crate::SIZE_PAGE));

const MEMORY_GLOBAL_OWNED_TIGHT: usize = mem::size_of::<thread::Array<allocator::Owned<()>>>()
    + mem::size_of::<thread::Array<UnsafeCell<heap::Owned<size::Small>>>>()
    + mem::size_of::<thread::Array<UnsafeCell<heap::Owned<size::Large>>>>()
    + mem::size_of::<thread::Array<huge::Owned>>();

static MEMORY_GLOBAL_SLAB_TIGHT: AtomicI64 = AtomicI64::new(0);

static MEMORY_GLOBAL_DATA_TIGHT: AtomicI64 = AtomicI64::new(0);

thread_local! {
    static MEMORY_LOCAL_SLAB_TIGHT: Cell<i64> = Cell::new(0);
    static MEMORY_LOCAL_DATA_TIGHT: Cell<i64> = Cell::new(0);

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
pub(crate) fn record_allocate<B: size::Bracket>(size: u64, allocate: bool) {
    if !cfg!(feature = "stat-memory") {
        return;
    }

    let direction = match allocate {
        true => 1,
        false => -1,
    };

    // Hack: huge allocation
    if B::COUNT == 1 {
        MEMORY_LOCAL_DATA_TIGHT.set(
            MEMORY_LOCAL_DATA_TIGHT.get()
                + (size as i64 + mem::size_of::<huge::Descriptor>() as i64) * direction,
        );
    } else {
        MEMORY_LOCAL_DATA_TIGHT.set(MEMORY_LOCAL_DATA_TIGHT.get() + size as i64 * direction);
        MEMORY_LOCAL_SLAB_TIGHT.set(
            MEMORY_LOCAL_SLAB_TIGHT.get()
                + mem::size_of::<slab::Descriptor<B>>() as i64 * direction,
        );
    }

    if let Some(value) = update(&MEMORY_LOCAL_DATA_TIGHT, &MEMORY_GLOBAL_DATA_TIGHT) {
        eprintln!("MEMORY_GLOBAL_DATA_TIGHT:{}", value);
    }

    if let Some(value) = update(&MEMORY_LOCAL_SLAB_TIGHT, &MEMORY_GLOBAL_SLAB_TIGHT) {
        eprintln!("MEMORY_GLOBAL_SLAB_TIGHT:{}", value);
    }
}

fn dump_memory_global(id: usize) {
    if !cfg!(feature = "stat-memory") || id > 0 {
        return;
    }

    eprintln!("MEMORY_GLOBAL_SHARED_TIGHT:{}", MEMORY_GLOBAL_SHARED_TIGHT);
    eprintln!("MEMORY_GLOBAL_SHARED_LOOSE:{}", *MEMORY_GLOBAL_SHARED_LOOSE);

    eprintln!("MEMORY_GLOBAL_OWNED_TIGHT:{}", MEMORY_GLOBAL_OWNED_TIGHT);
    eprintln!("MEMORY_GLOBAL_OWNED_LOOSE:{}", *MEMORY_GLOBAL_OWNED_LOOSE);
}

fn update(local: &'static LocalKey<Cell<i64>>, global: &AtomicI64) -> Option<i64> {
    const UNCERTAINTY: i64 = 1 << 20;
    match local.get() {
        value if value.abs() < UNCERTAINTY => None,
        value => {
            let old = global.fetch_add(value, Ordering::Relaxed);
            local.set(0);
            Some(old + value)
        }
    }
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

fn dump_sizes(id: usize) {
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
