use core::marker::PhantomData;

use crate::atomic::Packed;
use crate::atomic::Version;
use crate::region;
use crate::thread;
use crate::Atomic;

pub(crate) struct Detectable<T>(Atomic<State<T>>);

#[derive(Copy, Clone)]
pub(crate) struct State<T> {
    value: u64,
    _type: PhantomData<T>,
}

impl<T: Packed + Copy> State<T> {
    fn new(id: thread::Id, version: Version, inner: T) -> Self {
        Self {
            value: (id.pack() << 48) | (version.pack() << 32) | inner.pack(),
            _type: PhantomData,
        }
    }

    fn inner(&self) -> T {
        Packed::unpack(self.value as u32 as u64)
    }

    fn version(&self) -> Version {
        Packed::unpack(self.value >> 32)
    }

    fn id(&self) -> Option<thread::Id> {
        Packed::unpack(self.value >> 48)
    }
}

unsafe impl<T: Packed + Copy> Packed for State<T> {
    const BITS: u8 = 64;

    fn pack(&self) -> u64 {
        self.value
    }

    fn unpack(value: u64) -> Self {
        Self {
            value,
            _type: PhantomData,
        }
    }
}

impl<T: Packed + Copy> Detectable<T> {
    pub(crate) fn load(&self, help: &thread::Array<Help>) -> T {
        let old = self.0.load();
        self.notify(help, old);
        old.inner()
    }

    fn notify(&self, help: &thread::Array<Help>, state: State<T>) {
        if let Some(id) = state.id() {
            let version = state.version();
            if help[id].must_notify(version) {
                crate::flush(&self.0, false);
                crate::fence();
                help[id].notify(version);
            }
        }
    }

    pub(crate) fn update<F>(
        &self,
        help: &thread::Array<Help>,
        id: thread::Id,
        meta: &mut region::owned::Meta,
        mut next: F,
    ) -> Option<T>
    where
        F: FnMut(T, Version) -> Option<(T, region::owned::State)>,
    {
        let mut old = self.0.load();
        let version = help[id].peek().next();
        help[id].prepare(version);

        crate::flush(&help[id], false);
        crate::fence();

        loop {
            self.notify(help, old);

            let (new, log) = next(old.inner(), version)?;
            meta.state.store(Some(log));
            crate::flush(&meta.state, false);

            match self.0.compare_exchange(old, State::new(id, version, new)) {
                Ok(_) => break Some(old.inner()),
                Err(next) => old = next,
            }
        }
    }
}

pub(crate) struct Help(Atomic<u64>);

impl Help {
    const FLAG: u64 = 1 << 63;

    pub(crate) fn peek(&self) -> Version {
        Version::unpack(self.0.load())
    }

    pub(crate) fn detect(&self) -> (Version, bool) {
        let value = self.0.load();
        (Version::unpack(value), value & Self::FLAG > 0)
    }

    pub(crate) fn prepare(&self, version: Version) {
        self.0.store(version.pack());
    }

    pub(crate) fn must_notify(&self, version: Version) -> bool {
        let (current, notified) = self.detect();
        current == version && !notified
    }

    pub(crate) fn notify(&self, version: Version) {
        let _ = self
            .0
            .compare_exchange(version.pack(), version.pack() | Self::FLAG);

        crate::flush(self, true);
    }
}
