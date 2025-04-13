use core::cell::UnsafeCell;
use core::mem;

use crate::bitset::BitSet;
use crate::size;
use crate::size::Bracket as _;
use crate::slab;
use crate::Atomic;

pub(crate) const SIZE_METADATA: usize = 2 * mem::size_of::<u64>() + mem::size_of::<Owner>();

#[repr(C, align(64))]
pub(crate) struct Local<B: size::Bracket> {
    pub(crate) next: Atomic<Option<slab::Index<B>>>,
    pub(crate) class: Atomic<B>,
    pub(crate) owner: Owner,
    pub(crate) free: UnsafeCell<B::BitSet>,
}

unsafe impl<B: size::Bracket> Sync for Local<B> {}

impl<B: size::Bracket> Local<B> {
    pub(crate) fn initialize(local: *mut Self, class: B) {
        let index = ribbit::private::pack(class) as usize;
        let owner = unsafe { (*core::ptr::addr_of!((*local).owner)).load() };

        static SMALL: [Local<size::Small>; size::Small::COUNT] = cache_small();
        static LARGE: [Local<size::Large>; size::Large::COUNT] = cache_large();

        match std::any::TypeId::of::<B>() {
            id if id == std::any::TypeId::of::<size::Small>() => unsafe {
                local.cast::<u8>().copy_from_nonoverlapping(
                    &SMALL[index] as *const _ as *const u8,
                    mem::size_of::<Local<size::Small>>(),
                )
            },
            id if id == std::any::TypeId::of::<size::Large>() => unsafe {
                local.cast::<u8>().copy_from_nonoverlapping(
                    &LARGE[index] as *const _ as *const u8,
                    mem::size_of::<Local<size::Large>>(),
                )
            },
            _ => unreachable!(),
        }

        unsafe { (*core::ptr::addr_of!((*local).owner)).store(owner) };
    }
}

macro_rules! generate {
    ($cache:ident, $new:ident, $ty:path) => {
        const fn $cache() -> [Local<$ty>; <$ty>::COUNT] {
            let mut locals = [const { $new() }; <$ty>::COUNT];
            let bit_sets = <$ty>::bit_sets();

            let mut i = 0;
            while i < locals.len() {
                locals[i].class = unsafe { Atomic::new_unchecked(i as u64) };
                locals[i].free = UnsafeCell::new(bit_sets[i]);
                i += 1
            }

            locals
        }

        const fn $new() -> Local<$ty> {
            Local {
                next: unsafe { Atomic::new_unchecked(0) },
                class: unsafe { Atomic::new_unchecked(0) },
                owner: Owner::new(),
                free: UnsafeCell::new(BitSet::new()),
            }
        }
    };
}

generate!(cache_small, new_small, size::Small);
generate!(cache_large, new_large, size::Large);

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
        pub(super) const fn new() -> Self {
            Self
        }

        pub(super) fn load(&self) -> Option<thread::Id> {
            None
        }

        pub(super) fn store(&self, _: Option<thread::Id>) {}

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
        pub(super) const fn new() -> Self {
            Self(AtomicU64::new(0))
        }

        pub(super) fn load(&self) -> Option<thread::Id> {
            match self.0.load(Ordering::Relaxed) {
                0 => None,
                id => Some(unsafe { thread::Id::new(id.trailing_zeros() as u16) }),
            }
        }

        pub(super) fn store(&self, id: Option<thread::Id>) {
            let bit = id.map(|id| 1 << u16::from(id)).unwrap_or(0);
            self.0.store(bit, Ordering::Relaxed);
        }

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
