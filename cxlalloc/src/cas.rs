use crate::allocator;
use crate::atomic::Version;
use crate::coherence::flush;
use crate::coherence::sfence;
use crate::coherence::Invalidate;
use crate::recover;
use crate::recover::HeapState;
use crate::thread;
use crate::Atomic;
use std::fmt::Debug;

pub(crate) struct Detectable<T>(Atomic<State<T>>);

#[ribbit::pack(size = 64)]
pub(crate) struct State<T> {
    #[ribbit(size = 16)]
    id: Option<thread::Id>,

    #[ribbit(size = 16)]
    version: Version,

    #[ribbit(size = 32)]
    inner: T,
}

impl<T: ribbit::Pack<Loose = u32> + Debug> Detectable<T> {
    pub(crate) fn load(&self, help: &help::Array) -> T {
        let old = self.0.load();
        self.help(help, old);
        old.inner()
    }

    pub(crate) fn update<F, B>(&self, context: &mut allocator::Context, mut next: F) -> Option<T>
    where
        F: FnMut(T, Version) -> Option<(T, HeapState<B>)>,
        recover::State: From<HeapState<B>>,
    {
        let mut old = self.0.load();
        let version = context.help[context.id].peek().next();

        if cfg!(feature = "recover-cas") {
            context.help[context.id].prepare(version);

            // Must wait for persistence
            flush(&context.help[context.id], Invalidate::No);
            sfence();
        }

        loop {
            self.help(context.help, old);

            let (new, log) = next(old.inner(), version)?;

            // Unsync because following compare-exchange is serializing
            context.log_unsync(log);

            match self
                .0
                .compare_exchange(old, State::new(Some(context.id), version, new))
            {
                Ok(_) => break Some(old.inner()),
                Err(next) => old = next,
            }
        }
    }

    fn help(&self, help: &help::Array, state: State<T>) {
        if !cfg!(feature = "recover-cas") {
            return;
        }

        if let Some(id) = state.id() {
            let version = state.version();
            if help[id].must_help(version) {
                flush(&self.0, Invalidate::No);
                // Notify is a CAS, which will serialize the flush
                help[id].help(version);
            }
        }
    }
}

pub(crate) struct Help(Atomic<Inner>);

#[ribbit::pack(size = 17)]
pub(crate) struct Inner {
    #[ribbit(size = 16)]
    version: Version,
    helped: bool,
}

impl Help {
    fn peek(&self) -> Version {
        self.0.load().version()
    }

    fn detect(&self) -> (Version, bool) {
        let inner = self.0.load();
        (inner.version(), inner.helped())
    }

    fn prepare(&self, version: Version) {
        self.0.store(Inner::new(version, false))
    }

    fn must_help(&self, version: Version) -> bool {
        let (current, notified) = self.detect();
        current == version && !notified
    }

    fn help(&self, version: Version) {
        let _ = self
            .0
            .compare_exchange(Inner::new(version, false), Inner::new(version, true));

        flush(self, Invalidate::No);
    }
}

pub(crate) mod help {
    use core::ops::Index;

    #[repr(transparent)]
    pub(crate) struct Array(crate::thread::Array<super::Help>);

    impl Index<crate::thread::Id> for Array {
        type Output = super::Help;

        fn index(&self, index: crate::thread::Id) -> &Self::Output {
            &self.0[index]
        }
    }
}
