use core::alloc::Layout;
use core::alloc::LayoutError;
use core::marker::PhantomData;
use core::num::NonZeroU32;
use core::num::NonZeroU64;
use core::ptr::NonNull;

use crate::bitset::Bit;
use crate::raw::Page;
use crate::size;
use crate::slab;

#[derive(Clone)]
pub(crate) struct Data<'raw, B> {
    pub(crate) base: NonNull<Page>,
    _raw: PhantomData<&'raw ()>,
    _bracket: PhantomData<B>,
}

impl<B> Data<'_, B>
where
    B: size::Bracket,
{
    pub(crate) fn new(base: NonNull<Page>) -> Self {
        Self {
            base: unsafe { base.byte_sub(B::SIZE_SLAB) },
            _raw: PhantomData,
            _bracket: PhantomData,
        }
    }

    pub(crate) fn layout(slab_count: usize) -> Result<Layout, LayoutError> {
        Layout::array::<u8>(B::SIZE_SLAB * slab_count)
    }

    pub(crate) fn offset_to_pointer<T>(&self, offset: Offset<B>) -> NonNull<T> {
        unsafe { self.base.byte_add(NonZeroU64::from(offset).get() as usize) }.cast()
    }

    pub(crate) fn offset_to_offset(&self, offset: usize) -> Offset<B> {
        let offset = offset + B::SIZE_SLAB;
        NonZeroU64::new(offset as u64).map(Offset::new).unwrap()
    }

    pub(crate) fn pointer_to_offset<T>(&self, pointer: NonNull<T>) -> Option<Offset<B>> {
        (pointer.as_ptr() as u64)
            .checked_sub(self.base.as_ptr() as u64)
            .and_then(NonZeroU64::new)
            .map(Offset::new)
    }
}

#[repr(transparent)]
#[derive(ribbit::Pack, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[ribbit(size = 64, nonzero)]
pub(crate) struct Offset<B> {
    #[ribbit(size = 0)]
    _bracket: PhantomData<B>,
    value: NonZeroU64,
}

impl<B> Copy for Offset<B> {}
impl<B> Clone for Offset<B> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<B> core::hash::Hash for Offset<B> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.value.hash(state)
    }
}

impl<B> Offset<B> {
    const fn new(value: NonZeroU64) -> Self {
        Self {
            value,
            _bracket: PhantomData,
        }
    }
}

impl<B: size::Bracket> Offset<B> {
    pub(crate) fn from_block(class: B, slab: slab::Index<B>, block: Bit) -> Self {
        debug_assert!(
            u64::from(block) <= class.count(),
            "{:?} {:?} {} {}",
            class,
            block,
            u64::from(block),
            class.count(),
        );
        NonZeroU64::new(
            NonZeroU32::from(slab).get() as u64 * (B::SIZE_SLAB as u64)
                + u64::from(block) * class.size(),
        )
        .map(Self::new)
        .unwrap()
    }

    pub(crate) fn into_block(self, class: B) -> Bit {
        unsafe {
            let block = self.value.get() % B::SIZE_SLAB as u64 / class.size();
            debug_assert!(
                block <= class.count(),
                "{:?} {:?} {}",
                class,
                block,
                class.count(),
            );
            Bit::from_loose(block as u16)
        }
    }

    pub(crate) fn into_index(self) -> slab::Index<B> {
        slab::Index::from(self)
    }
}

impl<B: size::Bracket> From<slab::Index<B>> for Offset<B> {
    fn from(index: slab::Index<B>) -> Self {
        NonZeroU64::new(NonZeroU32::from(index).get() as u64 * B::SIZE_SLAB as u64)
            .map(Self::new)
            .unwrap()
    }
}

impl<B> From<Offset<B>> for NonZeroU64 {
    fn from(offset: Offset<B>) -> Self {
        offset.value
    }
}

impl<B: size::Bracket> From<Offset<B>> for u64 {
    fn from(offset: Offset<B>) -> Self {
        offset.value.get() - B::SIZE_SLAB as u64
    }
}

impl<B> core::ops::Add<u64> for Offset<B> {
    type Output = Self;
    fn add(self, rhs: u64) -> Self::Output {
        Self::new(self.value.checked_add(rhs).unwrap())
    }
}
