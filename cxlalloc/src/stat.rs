use core::cell::Cell;
use core::cell::UnsafeCell;
use core::fmt::Display;
use core::fmt::Write as _;
use core::iter;
use core::marker::PhantomData;
use core::mem;
use core::sync::atomic::AtomicI64;
use core::sync::atomic::Ordering;
use std::sync::LazyLock;

use crate::allocator;
use crate::heap;
use crate::huge;
use crate::size;
use crate::size::Bracket as _;
use crate::slab;
use crate::thread;
use crate::Raw;
use crate::COUNT_THREAD;

pub fn dump(id: usize) {
    let thread = unsafe { thread::Id::new(id as u16) };
    HEAP_SMALL.finalize(thread);
    HEAP_LARGE.finalize(thread);
    HEAP_HUGE.finalize(thread);
    dump_memory_global(id);
    dump_sizes(id);
}

#[repr(align(64))]
struct Counter(AtomicI64);

impl Counter {
    fn update(&self, delta: i64) -> i64 {
        let prev = self.0.load(Ordering::Relaxed);
        let next = prev + delta;
        self.0.store(next, Ordering::Relaxed);
        next
    }

    fn reset(&self) {
        self.0.store(0, Ordering::Relaxed);
    }
}

struct Sloppy {
    bracket: &'static str,
    name: &'static str,
    size: Option<u64>,
    buffer: thread::Array<Counter>,
}

impl Sloppy {
    const fn new(bracket: &'static str, name: &'static str, size: Option<u64>) -> Self {
        Self {
            bracket,
            name,
            size,
            buffer: thread::Array([const { Counter(AtomicI64::new(0)) }; COUNT_THREAD + 1]),
        }
    }

    fn update(&self, id: thread::Id, delta: i64) {
        self.apply::<{ 1 << 12 }>(id, delta)
    }

    fn finalize(&self, id: thread::Id) {
        self.apply::<0>(id, 0)
    }

    #[inline]
    fn apply<const THRESHOLD: i64>(&self, id: thread::Id, delta: i64) {
        if !cfg!(feature = "stat-memory") {
            return;
        }

        let value = self.buffer[id].update(delta);
        if value.abs() < THRESHOLD {
            return;
        }

        self.buffer[id].reset();
        let size = self.size.as_ref();
        let size = match size {
            None => &"" as &dyn Display,
            Some(size) => size,
        };

        let now = now();
        eprintln!(
            "{},{},{},{},{},{}",
            now, self.bracket, self.name, id, size, value
        );
    }
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
    Claim { class: B },
}

struct Heap<B: size::Bracket> {
    application: size::Array<B, Sloppy>,
    global_unsized: Sloppy,
    local_unsized: Sloppy,
    local_sized: size::Array<B, Sloppy>,
    detached: size::Array<B, Sloppy>,

    data: Sloppy,
    slab_local: Sloppy,
    slab_remote: Sloppy,
}

static HEAP_SMALL: Heap<size::Small> = Heap {
    application: small_by_size("application"),
    global_unsized: Sloppy::new("small", "global_unsized", None),
    local_unsized: Sloppy::new("small", "local_unsized", None),
    local_sized: small_by_size("local_sized"),
    detached: small_by_size("detached"),

    data: Sloppy::new("small", "data", None),
    slab_local: Sloppy::new("small", "slab_local", None),
    slab_remote: Sloppy::new("small", "slab_remote", None),
};

const fn small_by_size(name: &'static str) -> size::Array<size::Small, Sloppy> {
    let mut counters = [const { Sloppy::new("small", "", None) }; size::Small::COUNT + 1];
    let mut i = 0;
    while i < counters.len() {
        counters[i].name = name;
        counters[i].size = Some(size::Small::from_index(i).size());
        i += 1;
    }
    size::Array {
        inner: counters,
        _bracket: PhantomData,
    }
}

static HEAP_LARGE: Heap<size::Large> = Heap {
    application: large_by_size("application"),
    global_unsized: Sloppy::new("large", "global_unsized", None),
    local_unsized: Sloppy::new("large", "local_unsized", None),
    local_sized: large_by_size("local_sized"),
    detached: large_by_size("detached"),

    data: Sloppy::new("large", "data", None),
    slab_local: Sloppy::new("large", "slab_local", None),
    slab_remote: Sloppy::new("large", "slab_remote", None),
};

const fn large_by_size(name: &'static str) -> size::Array<size::Large, Sloppy> {
    let mut counters = [const { Sloppy::new("large", "", None) }; size::Large::COUNT];
    let mut i = 0;
    while i < counters.len() {
        counters[i].name = name;
        counters[i].size = Some(size::Large::from_index(i).size());
        i += 1;
    }
    size::Array {
        inner: counters,
        _bracket: PhantomData,
    }
}

static HEAP_HUGE: Sloppy = Sloppy::new("huge", "application", None);

trait Record {
    fn finalize(&self, id: thread::Id);
    fn application(&self, class: u8) -> &Sloppy;
    fn global_unsized(&self) -> &Sloppy;
    fn local_unsized(&self) -> &Sloppy;
    fn local_sized(&self, class: u8) -> &Sloppy;
    fn detached(&self, class: u8) -> &Sloppy;
    fn data(&self) -> &Sloppy;
    fn slab_local(&self) -> &Sloppy;
    fn slab_remote(&self) -> &Sloppy;
}

impl Record for Sloppy {
    fn finalize(&self, id: thread::Id) {
        self.application(0).finalize(id)
    }

    fn application(&self, _: u8) -> &Sloppy {
        &HEAP_HUGE
    }

    fn global_unsized(&self) -> &Sloppy {
        unreachable!()
    }

    fn local_unsized(&self) -> &Sloppy {
        unreachable!()
    }

    fn local_sized(&self, _: u8) -> &Sloppy {
        unreachable!()
    }

    fn detached(&self, _: u8) -> &Sloppy {
        unreachable!()
    }

    fn data(&self) -> &Sloppy {
        unreachable!()
    }

    fn slab_local(&self) -> &Sloppy {
        unreachable!()
    }

    fn slab_remote(&self) -> &Sloppy {
        unreachable!()
    }
}

impl<B: size::Bracket> Record for Heap<B> {
    fn finalize(&self, id: thread::Id) {
        self.application
            .iter()
            .map(|(_, counter)| counter)
            .chain(iter::once(&self.global_unsized))
            .chain(iter::once(&self.local_unsized))
            .chain(self.local_sized.iter().map(|(_, counter)| counter))
            .chain(self.detached.iter().map(|(_, counter)| counter))
            .chain(iter::once(&self.data))
            .chain(iter::once(&self.slab_remote))
            .chain(iter::once(&self.slab_local))
            .for_each(|counter| counter.finalize(id))
    }

    fn application(&self, class: u8) -> &Sloppy {
        &self.application.inner.as_ref()[class as usize]
    }

    fn global_unsized(&self) -> &Sloppy {
        &self.global_unsized
    }

    fn local_unsized(&self) -> &Sloppy {
        &self.local_unsized
    }

    fn local_sized(&self, class: u8) -> &Sloppy {
        &self.local_sized.inner.as_ref()[class as usize]
    }

    fn detached(&self, class: u8) -> &Sloppy {
        &self.detached.inner.as_ref()[class as usize]
    }

    fn data(&self) -> &Sloppy {
        &self.data
    }

    fn slab_local(&self) -> &Sloppy {
        &self.slab_local
    }

    fn slab_remote(&self) -> &Sloppy {
        &self.slab_remote
    }
}

#[inline]
pub(crate) fn record<B: size::Bracket>(id: thread::Id, event: Event<B>) {
    if !cfg!(feature = "stat-memory") {
        return;
    }

    let recorder: &dyn Record = match B::INDEX {
        0 => &HEAP_SMALL,
        1 => &HEAP_LARGE,
        2 => &HEAP_HUGE,
        _ => unreachable!(),
    };

    let slab = B::SIZE_SLAB as i64;

    match event {
        Event::Allocate { size } => {
            match B::new(size as usize) {
                None => recorder.application(0),
                Some(class) => recorder.application(class.pack()),
            }
            .update(id, size as i64);
        }
        Event::Bump => {
            let batch = crate::BATCH_BUMP_POP.load(Ordering::Relaxed) as i64;
            let size = slab * batch;
            recorder.local_unsized().update(id, size);
            recorder.data().update(id, size);
            recorder
                .slab_local()
                .update(id, mem::size_of::<slab::Local<B>>() as i64 * batch);
            recorder
                .slab_remote()
                .update(id, mem::size_of::<slab::Remote<B>>() as i64 * batch);
        }
        Event::GlobalToUnsized => {
            recorder.global_unsized().update(id, -slab);
            recorder.local_unsized().update(id, slab);
        }
        Event::UnsizedToSized { class } => {
            recorder.local_unsized().update(id, -slab);
            recorder.local_sized(class.pack()).update(id, slab);
        }

        Event::Free { size } => {
            match B::new(size as usize) {
                None => recorder.application(0),
                Some(class) => recorder.application(class.pack()),
            }
            .update(id, -(size as i64));
        }
        Event::SizedToUnsized { class } => {
            recorder.local_sized(class.pack()).update(id, -slab);
            recorder.local_unsized().update(id, slab);
        }
        Event::UnsizedToGlobal => {
            let batch = crate::BATCH_GLOBAL_PUSH.load(Ordering::Relaxed) as i64;
            recorder.local_unsized().update(id, -slab * batch);
            recorder.global_unsized().update(id, slab * batch);
        }

        Event::Detach { class } => {
            recorder.local_sized(class.pack()).update(id, -slab);
            recorder.detached(class.pack()).update(id, slab);
        }
        Event::Attach { class } => {
            recorder.detached(class.pack()).update(id, -slab);
            recorder.local_sized(class.pack()).update(id, slab);
        }
        Event::Claim { class } => {
            recorder.detached(class.pack()).update(id, -slab);
            recorder.local_unsized().update(id, slab);
        }
    }
}

fn now() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_micros()
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
