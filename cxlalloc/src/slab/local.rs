use core::cell::UnsafeCell;
use core::mem;

use crate::size;
use crate::slab;
use crate::Atomic;

pub(crate) const SIZE_METADATA: usize = 2 + mem::size_of::<Owner>();

#[repr(C, align(64))]
pub(crate) struct Local<B: size::Bracket> {
    pub(crate) next: Atomic<Option<slab::Index<B>>>,
    pub(crate) class: Atomic<B>,
    pub(crate) owner: Owner,
    pub(crate) free: UnsafeCell<B::BitSet>,
}

#[cfg(feature = "validate")]
pub(crate) type Owner = validate::Owner;

#[cfg(not(feature = "validate"))]
pub(crate) type Owner = assume::Owner;

#[cfg_attr(feature = "validate", expect(dead_code))]
mod assume {
    use crate::thread;

    pub(crate) struct Owner;

    impl Owner {
        #[inline]
        pub fn transfer(
            &self,
            _: Option<thread::Id>,
            _: Option<thread::Id>,
        ) -> Result<(), Option<thread::Id>> {
            Ok(())
        }
    }
}

#[cfg_attr(not(feature = "validate"), expect(dead_code))]
mod validate {
    use core::sync::atomic::AtomicU64;
    use core::sync::atomic::Ordering;

    use crate::thread;

    pub(crate) struct Owner(AtomicU64);

    impl Owner {
        #[inline]
        pub fn transfer(
            &self,
            old: Option<thread::Id>,
            new: Option<thread::Id>,
        ) -> Result<(), Option<thread::Id>> {
            let old_bit = old.map(|id| 1 << u16::from(id)).unwrap_or(0);
            let new_bit = new.map(|id| 1 << u16::from(id)).unwrap_or(0);

            self.0
                .compare_exchange(old_bit, new_bit, Ordering::AcqRel, Ordering::Acquire)
                .map(drop)
                .map_err(|old| match old {
                    0 => None,
                    _ => Some(unsafe { thread::Id::new(old.trailing_zeros() as u16) }),
                })
        }
    }
}
