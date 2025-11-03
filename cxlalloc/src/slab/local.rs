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

// pub(crate) trait Cache<B: size::Bracket>: Sized + 'static {
//     const CACHE: NonNull<size::Array<B, Self>>;
// }
//
// static SMALL: size::Array<size::Small, Local<size::Small>> = cache_small();
// impl Cache<size::Small> for Local<size::Small> {
//     const CACHE: NonNull<size::Array<size::Small, Self>> =
//         unsafe { NonNull::new_unchecked(&SMALL as *const _ as *mut _) };
// }
//
// static LARGE: size::Array<size::Large, Local<size::Large>> = cache_large();
// impl Cache<size::Large> for Local<size::Large> {
//     const CACHE: NonNull<size::Array<size::Large, Self>> =
//         unsafe { NonNull::new_unchecked(&LARGE as *const _ as *mut _) };
// }
//
// macro_rules! generate {
//     ($cache:ident, $new:ident, $ty:path) => {
//         const fn $cache() -> size::Array<$ty, Local<$ty>> {
//             let mut locals = [const { $new() }; <$ty>::COUNT];
//             let bit_sets = <$ty>::bit_sets();
//
//             let mut i = 0;
//             while i < locals.len() {
//                 locals[i].class = <$ty>::from_index(i as u8);
//                 locals[i].free = bit_sets[i];
//                 i += 1
//             }
//
//             unsafe { core::mem::transmute(locals) }
//         }
//
//         const fn $new() -> Local<$ty> {
//             Local {
//                 class: <$ty>::from_index(0),
//                 owner: Owner::new(),
//                 free: BitSet::new(),
//             }
//         }
//     };
// }
//
// generate!(cache_small, new_small, size::Small);
// generate!(cache_large, new_large, size::Large);
