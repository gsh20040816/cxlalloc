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
use crate::thread;
use crate::Raw;

pub fn dump(id: usize) {
    dump_memory_global(id);
    dump_counters(id);
    dump_sizes(id);
}

struct Sloppy {
    name: &'static str,
    local: &'static LocalKey<Cell<i64>>,
    global: &'static AtomicI64,
}

macro_rules! define_sloppy {
    ($($name:ident),* $(,)?) => {
        $(
            #[allow(non_snake_case)]
            fn $name(&self) -> &'static Sloppy {
                thread_local! {
                    static LOCAL: Cell<i64> = const { Cell::new(0) };
                }
                static GLOBAL: AtomicI64 = AtomicI64::new(0);
                static SLOPPY: Sloppy = Sloppy {
                    name: stringify!($name),
                    local: &LOCAL,
                    global: &GLOBAL,
                };
                &SLOPPY
            }
        )*
    };
}

impl Sloppy {
    fn update(&self, value: i64) {
        const THRESHOLD: i64 = 1 << 3;
        self.local.set(self.local.get() + value);
        if self.local.get().abs() < THRESHOLD {
            return;
        }

        let local = self.local.get();
        let global = self.global.fetch_add(local, Ordering::Relaxed);
        self.local.set(0);
        let now = std::time::SystemTime::now();
        eprintln!(
            "{}:{}={}",
            self.name,
            now.duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis(),
            global + local
        );
    }
}

macro_rules! define_counter {
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

define_counter![
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
fn inc(counter: &'static LocalKey<Cell<usize>>) {
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

pub(crate) enum Event<B: size::Bracket> {
    Bump,

    GlobalToUnsized,

    Allocate { size: u64 },

    UnsizedToSized { class: B },

    Free { size: u64 },
    SizedToUnsized { class: B },
    UnsizedToGlobal,

    Detach { class: B },
    Attach { class: B },
    Claim,
}

struct Global;

impl Global {
    define_sloppy![APPLICATION];
}

trait Heap {
    define_sloppy![GLOBAL_UNSIZED, LOCAL_UNSIZED, LOCAL_SIZED, DETACHED];
}

struct Small;
struct Large;

impl Heap for Small {}
impl Heap for Large {}

#[inline]
pub(crate) fn record<B: size::Bracket>(event: Event<B>) {
    let heap = match core::any::type_name::<B>() {
        name if name.contains("Small") => &Small as &dyn Heap,
        name if name.contains("Large") => &Large,
        _ => todo!(),
    };

    match event {
        Event::Allocate { size } => {
            inc(&ALLOCATE);
            Global.APPLICATION().update(size as i64);
        }
        Event::Bump => {
            inc(&BUMP);
            heap.LOCAL_UNSIZED()
                .update(B::SIZE_SLAB as i64 * crate::BATCH_BUMP_POP as i64);
        }
        Event::GlobalToUnsized => {
            heap.GLOBAL_UNSIZED().update(-(B::SIZE_SLAB as i64));
            heap.LOCAL_UNSIZED().update(B::SIZE_SLAB as i64);
        }
        Event::UnsizedToSized { class: _ } => {
            heap.LOCAL_UNSIZED().update(-(B::SIZE_SLAB as i64));
            heap.LOCAL_SIZED().update(B::SIZE_SLAB as i64);
        }

        Event::Free { size } => {
            inc(&FREE);
            Global.APPLICATION().update(-(size as i64));
        }
        Event::SizedToUnsized { class: _ } => {
            heap.LOCAL_SIZED().update(-(B::SIZE_SLAB as i64));
            heap.LOCAL_UNSIZED().update(B::SIZE_SLAB as i64);
        }
        Event::UnsizedToGlobal => {
            heap.LOCAL_UNSIZED()
                .update(-(B::SIZE_SLAB as i64 * crate::BATCH_GLOBAL_PUSH as i64));
            heap.GLOBAL_UNSIZED()
                .update(B::SIZE_SLAB as i64 * crate::BATCH_GLOBAL_PUSH as i64);
        }

        Event::Detach { class } => {
            heap.LOCAL_SIZED().update(-(B::SIZE_SLAB as i64));
            heap.DETACHED().update(B::SIZE_SLAB as i64);
        }
        Event::Attach { class } => {
            heap.DETACHED().update(-(B::SIZE_SLAB as i64));
            heap.LOCAL_SIZED().update(B::SIZE_SLAB as i64);
        }
        Event::Claim => {
            heap.DETACHED().update(-(B::SIZE_SLAB as i64));
            heap.LOCAL_UNSIZED().update(B::SIZE_SLAB as i64);
        }
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
