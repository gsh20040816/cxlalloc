use core::mem;

use crate::bitset::BitSet;
use crate::size;
use crate::size::Bracket as _;

pub(crate) const SIZE_METADATA: usize = mem::size_of::<u64>() + mem::size_of::<Owner>();

#[repr(C)]
pub(crate) struct Local<B: size::Bracket> {
    pub(crate) class: B,
    pub(crate) owner: Owner,
    pub(crate) free: B::BitSet,
}

unsafe impl<B: size::Bracket> Sync for Local<B> {}

impl<B: size::Bracket> Local<B>
where
    Local<B>: Cache<B>,
{
    pub(crate) fn initialize(&mut self, class: B) {
        let owner = self.owner.load();

        unsafe {
            (self as *mut Self).copy_from_nonoverlapping(&<Self as Cache<B>>::CACHE[class], 1);
        }

        self.owner.store(owner);
    }
}

pub(crate) trait Cache<B: size::Bracket>: Sized + 'static {
    const CACHE: &'static size::Array<B, Self>;
}

static SMALL: size::Array<size::Small, Local<size::Small>> = cache_small();
impl Cache<size::Small> for Local<size::Small> {
    const CACHE: &'static size::Array<size::Small, Self> = &SMALL;
}

static LARGE: size::Array<size::Large, Local<size::Large>> = cache_large();
impl Cache<size::Large> for Local<size::Large> {
    const CACHE: &'static size::Array<size::Large, Self> = &LARGE;
}

macro_rules! generate {
    ($cache:ident, $new:ident, $ty:path) => {
        const fn $cache() -> size::Array<$ty, Local<$ty>> {
            let mut locals = [const { $new() }; <$ty>::COUNT];
            let bit_sets = <$ty>::bit_sets();

            let mut i = 0;
            while i < locals.len() {
                locals[i].class = <$ty>::from_index(i as u8);
                locals[i].free = bit_sets[i];
                i += 1
            }

            unsafe { core::mem::transmute(locals) }
        }

        const fn $new() -> Local<$ty> {
            Local {
                class: <$ty>::from_index(0),
                owner: Owner::new(),
                free: BitSet::new(),
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

    #[derive(Clone)]
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

        pub(crate) fn is(&self, _: thread::Id) -> bool {
            true
        }

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
    use core::sync::atomic::Ordering;

    use ribbit::atomic::Atomic64;

    use crate::thread;

    pub(crate) struct Owner(Atomic64<Option<thread::Id>>);

    impl Owner {
        #[inline]
        pub(super) const fn new() -> Self {
            Self(Atomic64::new(None))
        }

        pub(crate) fn is(&self, id: thread::Id) -> bool {
            self.0.load(Ordering::Relaxed) == Some(id)
        }

        pub(super) fn load(&self) -> Option<thread::Id> {
            self.0.load(Ordering::Relaxed)
        }

        pub(super) fn store(&self, id: Option<thread::Id>) {
            self.0.store(id, Ordering::Relaxed)
        }

        #[inline]
        pub fn transfer(
            &self,
            old: Option<thread::Id>,
            new: Option<thread::Id>,
        ) -> Result<(), Option<thread::Id>> {
            self.0
                .compare_exchange(old, new, Ordering::AcqRel, Ordering::Acquire)
                .map(drop)
        }
    }
}
