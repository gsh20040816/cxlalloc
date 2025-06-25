#![cfg_attr(
    not(all(feature = "stat-event", feature = "stat-memory")),
    expect(dead_code)
)]

use core::sync::atomic::AtomicI64;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::Ordering;

pub fn dump(_id: usize) {}

// static SIZE_SHARED: LazyLock<usize> =
//     LazyLock::new(|| Raw::shared().0.get().next_multiple_of(crate::SIZE_PAGE));
//
// static SIZE_OWNED: LazyLock<usize> =
//     LazyLock::new(|| Raw::owned().0.get().next_multiple_of(crate::SIZE_PAGE));

pub(crate) mod process {
    use super::Report;

    #[allow(clippy::enum_variant_names)]
    #[derive(Copy, Clone)]
    pub(crate) enum Event {
        FaultSmall,
        FaultLarge,
        FaultHuge,
    }

    #[derive(Default)]
    pub(crate) struct Recorder {
        #[cfg(feature = "stat-event")]
        fault_small: super::Counter,

        #[cfg(feature = "stat-event")]
        fault_large: super::Counter,

        #[cfg(feature = "stat-event")]
        fault_huge: super::Counter,
    }

    #[cfg(not(feature = "stat-event"))]
    impl Recorder {
        #[inline]
        pub(crate) fn record(&self, _event: Event) {}

        #[inline]
        pub(crate) fn report(&self) -> impl Iterator<Item = Report> + '_ {
            core::iter::empty()
        }
    }

    #[cfg(feature = "stat-event")]
    impl Recorder {
        pub(crate) fn record(&self, event: Event) {
            let counter = match event {
                Event::FaultSmall => &self.fault_small,
                Event::FaultLarge => &self.fault_large,
                Event::FaultHuge => &self.fault_huge,
            };

            counter.increment_atomic();
        }

        pub(crate) fn report(&self) -> impl Iterator<Item = Report> + '_ {
            [
                ("small", &self.fault_small),
                ("large", &self.fault_large),
                ("huge", &self.fault_huge),
            ]
            .into_iter()
            .map(|(heap, counter)| Report {
                heap,
                event: "fault",
                class: None,
                count: counter.load(),
            })
        }
    }
}

pub(crate) mod thread {
    use core::marker::PhantomData;
    use core::mem;
    use core::sync::atomic::Ordering;

    use crate::size;
    use crate::slab;
    use crate::thread;

    use super::Counter;
    use super::Report;
    use super::Sloppy;

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
        Disown { class: B },
        Attach { class: B },
        Claim { class: B },
    }

    pub(crate) struct Recorder<B: size::Bracket> {
        #[cfg(feature = "stat-event")]
        event: Box<thread::Array<Box<EventRecorder<B>>>>,

        #[cfg(feature = "stat-memory")]
        memory: Box<thread::Array<Box<MemoryRecorder<B>>>>,

        _bracket: PhantomData<B>,
    }

    impl<B: size::Bracket> Default for Recorder<B> {
        fn default() -> Self {
            Self {
                #[cfg(feature = "stat-event")]
                event: Default::default(),

                #[cfg(feature = "stat-memory")]
                memory: Default::default(),

                _bracket: Default::default(),
            }
        }
    }

    impl<B: size::Bracket> Recorder<B> {
        pub(crate) fn report(&self, _id: thread::Id) -> impl Iterator<Item = Report> + '_ {
            #[cfg(not(feature = "stat-event"))]
            {
                core::iter::empty()
            }

            #[cfg(feature = "stat-event")]
            {
                self.event[_id].report()
            }
        }

        #[inline]
        pub(crate) fn record(&self, _id: thread::Id, _event: Event<B>) {
            #[cfg(feature = "stat-event")]
            self.event[_id].record(_event);

            #[cfg(feature = "stat-memory")]
            self.memory[_id].record::<{ 1 << 12 }, _>(_event, |event, size, value| {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|duration| duration.as_micros())
                    .unwrap_or(0);

                eprintln!(
                    "{},{},{},{},{},{}",
                    now,
                    B::NAME,
                    event,
                    _id,
                    match size.as_ref() {
                        None => &"" as &dyn core::fmt::Display,
                        Some(value) => value,
                    },
                    value
                );
            });
        }
    }

    struct EventRecorder<B: size::Bracket> {
        bump: Counter,
        global_to_unsized: Counter,
        allocate: size::Array<B, Counter>,
        unsized_to_sized: size::Array<B, Counter>,
        free: size::Array<B, Counter>,
        sized_to_unsized: size::Array<B, Counter>,
        unsized_to_global: Counter,
        detach: size::Array<B, Counter>,
        disown: size::Array<B, Counter>,
        attach: size::Array<B, Counter>,
        claim: size::Array<B, Counter>,
    }

    impl<B: size::Bracket> Default for EventRecorder<B> {
        // Avoid requiring `B: Default`
        fn default() -> Self {
            Self {
                bump: Default::default(),
                global_to_unsized: Default::default(),
                allocate: Default::default(),
                unsized_to_sized: Default::default(),
                free: Default::default(),
                sized_to_unsized: Default::default(),
                unsized_to_global: Default::default(),
                detach: Default::default(),
                disown: Default::default(),
                attach: Default::default(),
                claim: Default::default(),
            }
        }
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
                Event::Disown { class } => &self.disown[class],
                Event::Attach { class } => &self.attach[class],
                Event::Claim { class } => &self.claim[class],
            };

            counter.increment();
        }

        fn report(&self) -> impl Iterator<Item = Report> + '_ {
            [
                ("allocate", &self.allocate),
                ("unsized_to_sized", &self.unsized_to_sized),
                ("free", &self.free),
                ("sized_to_unsized", &self.sized_to_unsized),
                ("detach", &self.detach),
                ("disown", &self.disown),
                ("attach", &self.attach),
                ("claim", &self.claim),
            ]
            .into_iter()
            .flat_map(Self::report_array)
            .chain(
                [
                    ("bump", &self.bump),
                    ("global_to_unsized", &self.global_to_unsized),
                    ("unsized_to_global", &self.unsized_to_global),
                ]
                .into_iter()
                .map(Self::report_counter),
            )
        }

        fn report_counter((event, counter): (&'static str, &Counter)) -> Report {
            Report {
                heap: B::NAME,
                event,
                class: None,
                count: counter.load(),
            }
        }

        fn report_array<'a>(
            (event, array): (&'static str, &'a size::Array<B, Counter>),
        ) -> impl Iterator<Item = Report> + 'a {
            array
                .iter()
                .filter(|(class, _)| !class.is_zero())
                .map(move |(class, counter)| Report {
                    heap: B::NAME,
                    event,
                    class: match class.size() {
                        // HACK: special case huge allocation
                        u64::MAX => None,
                        size => Some(size),
                    },
                    count: counter.load(),
                })
        }
    }

    struct MemoryRecorder<B: size::Bracket> {
        data: Sloppy,
        slab_local: Sloppy,
        slab_remote: Sloppy,

        application: size::Array<B, Sloppy>,
        global_unsized: Sloppy,
        local_unsized: Sloppy,
        local_sized: size::Array<B, Sloppy>,
        detached: size::Array<B, Sloppy>,
        disowned: size::Array<B, Sloppy>,
    }

    impl<B: size::Bracket> Default for MemoryRecorder<B> {
        fn default() -> Self {
            Self {
                data: Default::default(),
                slab_local: Default::default(),
                slab_remote: Default::default(),
                application: Default::default(),
                global_unsized: Default::default(),
                local_unsized: Default::default(),
                local_sized: Default::default(),
                detached: Default::default(),
                disowned: Default::default(),
            }
        }
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
                        mem::size_of::<slab::Remote>() as i64 * batch,
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

                Event::Disown { class } => {
                    if let Some(value) = update(&self.detached[class], -slab) {
                        apply("detached", Some(class.size()), value);
                    }

                    if let Some(value) = update(&self.disowned[class], slab) {
                        apply("disowned", Some(class.size()), value);
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
                    if let Some(value) = update(&self.disowned[class], -slab) {
                        apply("disowned", Some(class.size()), value);
                    }

                    if let Some(value) = update(&self.local_unsized, slab) {
                        apply("local_unsized", None, value);
                    }
                }
            }
        }
    }
}

#[derive(Clone)]
#[cfg_attr(feature = "stat-event", derive(serde::Deserialize, serde::Serialize))]
pub struct Report {
    pub heap: &'static str,
    pub event: &'static str,
    pub class: Option<u64>,
    pub count: u64,
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
    fn increment_atomic(&self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }

    fn increment(&self) {
        let prev = self.0.load(Ordering::Relaxed);
        self.0.store(prev + 1, Ordering::Relaxed);
    }

    fn load(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
}
