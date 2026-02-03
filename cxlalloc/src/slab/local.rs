use core::cell::UnsafeCell;
use core::sync::atomic::Ordering;

use ribbit::atomic::Atomic16;
use ribbit::atomic::Atomic32;
use ribbit::atomic::Atomic8;

use crate::size;
use crate::slab;
use crate::thread;

pub(crate) const SIZE_METADATA: usize = 8;

#[repr(C)]
pub(crate) struct Local<B: size::Bracket> {
    pub(crate) next: Atomic32<Option<slab::Index<B>>>,
    owner: Atomic16<Option<thread::Id>>,
    pub(crate) class: Atomic8<B>,
    pub(crate) free: UnsafeCell<B::BitSet>,
}

unsafe impl<B: size::Bracket> Sync for Local<B> {}

impl<B: size::Bracket> Local<B> {
    pub(crate) fn owner(&self) -> Option<thread::Id> {
        self.owner.load(Ordering::Relaxed)
    }

    pub(crate) fn own(&self, id: thread::Id) {
        // FIXME: can't assert here in crash tests
        validate_eq!(self.owner(), None);
        self.owner.store(Some(id), Ordering::Relaxed);
    }

    pub(crate) fn steal(&self, id: thread::Id) {
        self.owner.store(Some(id), Ordering::Relaxed);
    }

    pub(crate) fn disown(&self, id: thread::Id) {
        validate_eq!(self.owner(), Some(id));
        self.owner.store(None, Ordering::Relaxed);
    }
}
