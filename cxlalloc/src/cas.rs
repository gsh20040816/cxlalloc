use core::sync::atomic::Ordering;

use crate::allocator;
use crate::cache;
use crate::recover;
use crate::recover::HeapState;
use crate::thread;

#[cfg(feature = "cxl-mcas")]
type Atomic<T> = crate::mcas::Atomic<T>;

#[cfg(not(feature = "cxl-mcas"))]
type Atomic<T> = ribbit::atomic::Atomic64<T>;

pub(crate) struct Detectable<T>(Atomic<State<T>>);

#[derive(ribbit::Pack, Copy, Clone)]
#[ribbit(size = 64)]
pub(crate) struct State<T> {
    #[ribbit(size = 16)]
    id: Option<thread::Id>,

    #[ribbit(size = 16)]
    version: Version,

    #[ribbit(size = 32)]
    inner: T,
}

impl<T: ribbit::Pack> Detectable<T> {
    pub(crate) fn load(&self, context: &allocator::Context, ordering: Ordering) -> T {
        let old = self.0.load(ordering);

        cache::flush(&self.0, cache::Invalidate::No);
        cache::fence();

        self.help(context, old);
        old.inner
    }

    pub(crate) fn store(&self, context: &mut allocator::Context, value: T, ordering: Ordering) {
        let old = self.0.load(Ordering::Relaxed);
        self.help(context, old);
        self.0.store(
            State {
                id: Some(context.id),
                version: Version::default(),
                inner: value,
            },
            ordering,
        );

        cache::flush(&self.0, cache::Invalidate::No);
        cache::fence();
    }

    pub(crate) fn update<F, B>(
        &self,
        context: &mut allocator::Context,
        success: Ordering,
        failure: Ordering,
        mut next: F,
    ) -> Option<T>
    where
        F: FnMut(T, Version) -> Option<(T, HeapState<B>)>,
        recover::State: From<HeapState<B>>,
    {
        let version = context.help.load(context.id, context.id).next();

        if cfg!(feature = "recover-cas") {
            context.help.store(context.id, context.id, version);
        }

        // Relaxed semantics are sufficient for helping, but the
        // data structure might need `Acquire` if following a pointer.
        let mut old = self.0.load(match success {
            Ordering::Acquire | Ordering::AcqRel => Ordering::Acquire,
            Ordering::Relaxed => Ordering::Relaxed,
            Ordering::Release | Ordering::SeqCst | _ => unreachable!(),
        });

        loop {
            self.help(context, old);

            let (new, log) = next(old.inner, version)?;

            // Unsync because following compare-exchange is serializing
            context.log_unsync(log);

            match self.0.compare_exchange(
                old,
                State {
                    id: Some(context.id),
                    version,
                    inner: new,
                },
                success,
                failure,
            ) {
                Err(next) => old = next,
                Ok(_) => {
                    cache::flush(&self.0, cache::Invalidate::No);
                    cache::fence();
                    return Some(old.inner);
                }
            }
        }
    }

    pub(crate) fn detect(&self, context: &mut allocator::Context, version: Version) -> bool {
        assert_eq!(context.help.load(context.id, context.id), version);

        // Ensure stores to help array are visible
        let state = self.0.load(Ordering::Acquire);

        // State hasn't been updated yet
        state.id == Some(context.id) && state.version == version
            // State has been observed by another thread before updating
            || context
                .help
                .0
                .iter()
                .map(|view| view[u16::from(context.id) as usize].load(Ordering::Relaxed))
                .filter(|observed| *observed == version)
                .count()
                > 1
    }

    fn help(&self, context: &allocator::Context, state: State<T>) {
        if !cfg!(feature = "recover-cas") {
            return;
        }

        let Some(id) = state.id else { return };
        let version = state.version;
        context.help.store(context.id, id, version);
    }
}

pub(crate) mod help {
    use core::sync::atomic::Ordering;

    use ribbit::atomic::Atomic16;

    use crate::cache;
    use crate::cas;
    use crate::thread;

    pub(crate) struct Array(
        pub(super) crate::thread::Array<[Atomic16<cas::Version>; crate::COUNT_THREAD]>,
    );

    impl Array {
        pub(super) fn load(&self, i: thread::Id, j: thread::Id) -> cas::Version {
            self.0[i][u16::from(j) as usize].load(Ordering::Relaxed)
        }

        pub(super) fn store(&self, i: thread::Id, j: thread::Id, new: cas::Version) {
            let version = &self.0[i][u16::from(j) as usize];

            version.store(new, Ordering::Relaxed);

            cache::flush(version, cache::Invalidate::No);
            cache::fence();

            cache::flush_cxl(version);
            cache::fence_cxl();
        }
    }
}

#[repr(transparent)]
#[derive(ribbit::Pack, Copy, Clone, Debug, Default, PartialEq, Eq)]
#[ribbit(size = 16)]
pub struct Version(u16);

impl Version {
    pub fn next(&self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}
