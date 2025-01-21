use crate::allocator;
use crate::atomic::Version;
use crate::recover::StateUnpacked;
use crate::thread;
use crate::Atomic;
use std::fmt::Debug;

pub(crate) struct Detectable<T>(Atomic<State<T>>);

#[ribbit::pack(size = 64)]
#[derive(Copy, Clone)]
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
        self.notify(help, old);
        old.inner()
    }

    pub(crate) fn update<F>(&self, context: &mut allocator::Context, mut next: F) -> Option<T>
    where
        F: FnMut(T, Version) -> Option<(T, StateUnpacked)>,
    {
        let mut old = self.0.load();
        let version = context.help[context.id].peek().next();

        if cfg!(feature = "recover-cas") {
            context.help[context.id].prepare(version);

            // Must wait for persistence
            crate::flush(&context.help[context.id], false);
            crate::fence();
        }

        loop {
            self.notify(context.help, old);

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

    fn notify(&self, help: &help::Array, state: State<T>) {
        if !cfg!(feature = "recover-cas") {
            return;
        }

        if let Some(id) = state.id() {
            let version = state.version();
            if help[id].must_notify(version) {
                crate::flush(&self.0, false);
                // Notify is a CAS, which will serialize the flush
                help[id].notify(version);
            }
        }
    }
}

pub(crate) struct Help(Atomic<Inner>);

#[ribbit::pack(size = 17)]
#[derive(Copy, Clone)]
pub(crate) struct Inner {
    #[ribbit(size = 16)]
    version: Version,
    helped: bool,
}

impl Help {
    pub(crate) fn peek(&self) -> Version {
        self.0.load().version()
    }

    pub(crate) fn detect(&self) -> (Version, bool) {
        let inner = self.0.load();
        (inner.version(), inner.helped())
    }

    pub(crate) fn prepare(&self, version: Version) {
        self.0.store(Inner::new(version, false))
    }

    pub(crate) fn must_notify(&self, version: Version) -> bool {
        let (current, notified) = self.detect();
        current == version && !notified
    }

    pub(crate) fn notify(&self, version: Version) {
        let _ = self
            .0
            .compare_exchange(Inner::new(version, false), Inner::new(version, true));

        crate::flush(self, false);
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
