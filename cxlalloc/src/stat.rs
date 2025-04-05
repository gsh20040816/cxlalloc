#![cfg_attr(
    not(all(feature = "stat-event", feature = "stat-memory")),
    expect(dead_code)
)]

use core::marker::PhantomData;
use core::mem;
use core::sync::atomic::AtomicI64;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::Ordering;

use crate::size;
use crate::slab;
use crate::thread;

pub fn dump(_id: usize) {}

// static SIZE_SHARED: LazyLock<usize> =
//     LazyLock::new(|| Raw::shared().0.get().next_multiple_of(crate::SIZE_PAGE));
//
// static SIZE_OWNED: LazyLock<usize> =
//     LazyLock::new(|| Raw::owned().0.get().next_multiple_of(crate::SIZE_PAGE));

#[derive(Copy, Clone)]
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

#[derive(Default)]
pub(crate) struct Recorder<B: size::Bracket> {
    #[cfg(feature = "stat-event")]
    event: thread::Array<EventRecorder<B>>,

    #[cfg(feature = "stat-memory")]
    memory: thread::Array<MemoryRecorder<B>>,

    _bracket: PhantomData<B>,
}

impl<B: size::Bracket> Recorder<B> {
    #[inline]
    pub(crate) fn record(&self, _id: thread::Id, _event: Event<B>) {
        #[cfg(feature = "stat-event")]
        self.event[_id].record(_event);

        #[cfg(feature = "stat-memory")]
        self.memory[_id].record::<{ 1 << 12 }, _>(_event, |name, size, value| {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_micros())
                .unwrap_or(0);

            eprintln!(
                "{},{},{},{},{},{}",
                now,
                B::NAME,
                name,
                _id,
                match size.as_ref() {
                    None => &"" as &dyn core::fmt::Display,
                    Some(value) => value,
                },
                value
            );
        });
    }

    pub(crate) fn report(
        &self,
        _id: thread::Id,
        _heap: &'static str,
    ) -> impl Iterator<Item = EventReport> + '_ {
        #[cfg(not(feature = "stat-event"))]
        {
            core::iter::empty()
        }

        #[cfg(feature = "stat-event")]
        {
            self.event[_id].report(_heap)
        }
    }
}

#[derive(Default)]
struct EventRecorder<B: size::Bracket> {
    bump: Counter,
    global_to_unsized: Counter,
    allocate: size::Array<B, Counter>,
    unsized_to_sized: size::Array<B, Counter>,
    free: size::Array<B, Counter>,
    sized_to_unsized: size::Array<B, Counter>,
    unsized_to_global: Counter,
    detach: size::Array<B, Counter>,
    attach: size::Array<B, Counter>,
    claim: size::Array<B, Counter>,
}

pub struct EventReport {
    pub heap: &'static str,
    pub name: &'static str,
    pub class: Option<u64>,
    pub count: u64,
}

impl<B: size::Bracket> EventRecorder<B> {
    fn record(&self, event: Event<B>) {
        let counter = match event {
            Event::Bump => &self.bump,
            Event::GlobalToUnsized => &self.global_to_unsized,
            Event::Allocate { size } => &self.allocate[B::new(size as usize).unwrap()],
            Event::UnsizedToSized { class } => &self.unsized_to_sized[class],
            Event::Free { size } => &self.free[B::new(size as usize).unwrap()],
            Event::SizedToUnsized { class } => &self.sized_to_unsized[class],
            Event::UnsizedToGlobal => &self.unsized_to_global,
            Event::Detach { class } => &self.detach[class],
            Event::Attach { class } => &self.attach[class],
            Event::Claim { class } => &self.claim[class],
        };

        counter.increment();
    }

    fn report(&self, heap: &'static str) -> impl Iterator<Item = EventReport> + '_ {
        [
            ("allocate", &self.allocate),
            ("unsized_to_sized", &self.unsized_to_sized),
            ("free", &self.free),
            ("sized_to_unsized", &self.sized_to_unsized),
            ("detach", &self.detach),
            ("attach", &self.attach),
            ("claim", &self.claim),
        ]
        .into_iter()
        .flat_map(move |(name, array)| Self::report_array(heap, name, array))
        .chain(
            [
                ("bump", &self.bump),
                ("global_to_unsized", &self.global_to_unsized),
                ("unsized_to_global", &self.unsized_to_global),
            ]
            .into_iter()
            .map(move |(name, counter)| Self::report_counter(heap, name, counter)),
        )
    }

    fn report_counter(heap: &'static str, name: &'static str, counter: &Counter) -> EventReport {
        EventReport {
            heap,
            name,
            class: None,
            count: counter.load(),
        }
    }

    fn report_array<'a>(
        heap: &'static str,
        name: &'static str,
        array: &'a size::Array<B, Counter>,
    ) -> impl Iterator<Item = EventReport> + 'a {
        array
            .iter()
            .filter(|(class, _)| !class.is_zero())
            .map(move |(class, counter)| EventReport {
                heap,
                name,
                class: match class.size() {
                    // HACK: special case huge allocation
                    u64::MAX => None,
                    size => Some(size),
                },
                count: counter.load(),
            })
    }
}

#[derive(Default)]
struct MemoryRecorder<B: size::Bracket> {
    data: Sloppy,
    slab_local: Sloppy,
    slab_remote: Sloppy,

    application: size::Array<B, Sloppy>,
    global_unsized: Sloppy,
    local_unsized: Sloppy,
    local_sized: size::Array<B, Sloppy>,
    detached: size::Array<B, Sloppy>,
}

impl<B: size::Bracket> MemoryRecorder<B> {
    #[inline]
    pub(crate) fn record<const THRESHOLD: i64, F: FnMut(&str, Option<u64>, i64)>(
        &self,
        event: Event<B>,
        mut apply: F,
    ) {
        let slab = B::SIZE_SLAB as i64;

        let update = Sloppy::apply::<THRESHOLD>;

        match event {
            Event::Allocate { size } => {
                let class = B::new(size as usize).unwrap();
                if let Some(value) = update(&self.application[class], size as i64) {
                    apply("application", Some(size), value);
                }
            }
            Event::Bump => {
                let batch = crate::BATCH_BUMP_POP.load(Ordering::Relaxed) as i64;
                let size = slab * batch;

                if let Some(value) = update(&self.local_unsized, size) {
                    apply("local_unsized", None, value);
                }

                if let Some(value) = update(&self.data, size) {
                    apply("data", None, value);
                }

                if let Some(value) = update(
                    &self.slab_local,
                    mem::size_of::<slab::Local<B>>() as i64 * batch,
                ) {
                    apply("slab_local", None, value);
                }

                if let Some(value) = update(
                    &self.slab_remote,
                    mem::size_of::<slab::Remote<B>>() as i64 * batch,
                ) {
                    apply("slab_remote", None, value);
                }
            }
            Event::GlobalToUnsized => {
                if let Some(value) = update(&self.global_unsized, -slab) {
                    apply("global_unsized", None, value);
                }

                if let Some(value) = update(&self.local_unsized, slab) {
                    apply("local_unsized", None, value);
                }
            }
            Event::UnsizedToSized { class } => {
                if let Some(value) = update(&self.local_unsized, -slab) {
                    apply("local_unsized", None, value);
                }

                if let Some(value) = update(&self.local_sized[class], slab) {
                    apply("local_sized", Some(class.size()), value);
                }
            }

            Event::Free { size } => {
                let class = B::new(size as usize).unwrap();

                if let Some(value) = update(&self.application[class], -(size as i64)) {
                    apply("application", Some(size), value);
                }
            }
            Event::SizedToUnsized { class } => {
                if let Some(value) = update(&self.local_sized[class], -slab) {
                    apply("local_sized", Some(class.size()), value);
                }

                if let Some(value) = update(&self.local_unsized, slab) {
                    apply("local_unsized", None, value);
                }
            }
            Event::UnsizedToGlobal => {
                let batch = crate::BATCH_GLOBAL_PUSH.load(Ordering::Relaxed) as i64;
                if let Some(value) = update(&self.local_unsized, -slab * batch) {
                    apply("local_unsized", None, value);
                }

                if let Some(value) = update(&self.global_unsized, slab * batch) {
                    apply("global_unsized", None, value);
                }
            }

            Event::Detach { class } => {
                if let Some(value) = update(&self.local_sized[class], -slab) {
                    apply("local_sized", Some(class.size()), value);
                }

                if let Some(value) = update(&self.detached[class], slab) {
                    apply("detached", Some(class.size()), value);
                }
            }
            Event::Attach { class } => {
                if let Some(value) = update(&self.detached[class], -slab) {
                    apply("detached", Some(class.size()), value);
                }

                if let Some(value) = update(&self.local_sized[class], slab) {
                    apply("local_sized", Some(class.size()), value);
                }
            }
            Event::Claim { class } => {
                if let Some(value) = update(&self.detached[class], -slab) {
                    apply("detached", Some(class.size()), value);
                }

                if let Some(value) = update(&self.local_unsized, slab) {
                    apply("local_unsized", None, value);
                }
            }
        }
    }
}

#[derive(Default)]
struct Sloppy(AtomicI64);

impl Sloppy {
    fn apply<const THRESHOLD: i64>(&self, delta: i64) -> Option<i64> {
        let prev = self.0.load(Ordering::Relaxed);
        let next = prev + delta;
        self.0.store(next, Ordering::Relaxed);

        if next.abs() < THRESHOLD {
            return None;
        }

        self.0.store(0, Ordering::Relaxed);
        Some(next)
    }
}

#[derive(Default)]
struct Counter(AtomicU64);

impl Counter {
    fn increment(&self) {
        let prev = self.0.load(Ordering::Relaxed);
        self.0.store(prev + 1, Ordering::Relaxed);
    }

    fn load(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
}
