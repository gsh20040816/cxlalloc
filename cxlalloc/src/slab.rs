use core::fmt::Debug;
use core::iter;
use core::marker::PhantomData;
use core::num::NonZeroU32;
use core::num::NonZeroUsize;
use core::ops::Range;
use core::ptr::NonNull;

use crate::atomic::NonZero;
use crate::atomic::Packed;
use crate::block;
use crate::raw;
use crate::size;
use crate::Atomic;
use crate::SIZE_SLAB;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub(crate) struct Index(NonZeroU32);

impl Index {
    pub(crate) fn from_offset(offset: NonZeroUsize) -> Self {
        u32::try_from(offset.get() / SIZE_SLAB)
            .map(NonZeroU32::new)
            .unwrap()
            .map(Self)
            .unwrap()
    }

    pub(crate) fn to_offset(self) -> NonZeroUsize {
        NonZeroUsize::new((self.0.get() as usize) * SIZE_SLAB).unwrap()
    }
}

unsafe impl Packed for Index {
    const BITS: u8 = 32;

    fn pack(&self) -> u64 {
        self.0.get() as u64
    }

    fn unpack(value: u64) -> Self {
        Self(unsafe { NonZeroU32::new_unchecked(value as u32) })
    }
}

unsafe impl NonZero for Index {}

pub(crate) struct Slice<'raw, M> {
    base: NonNull<Slab<M>>,
    _raw: PhantomData<&'raw raw::Region>,
}

impl<M> Slice<'_, M> {
    // Implementation detail: store minus one
    pub(crate) unsafe fn from_raw(region: &raw::Region, offset: usize) -> Self {
        let base = region
            .base()
            .byte_add(offset)
            .as_ptr()
            .cast::<Slab<M>>()
            .wrapping_sub(1);

        Self {
            base: NonNull::new(base).unwrap(),
            _raw: PhantomData,
        }
    }
}

impl Slice<'_, Owned> {
    // FIXME: type of `range` for integration with `meta::Shared::allocate`
    pub(crate) unsafe fn link(&self, range: Range<u32>) {
        let range = range
            .map(|index| index.checked_add(1))
            .map(Option::unwrap)
            .map(NonZeroU32::new)
            .map(Option::unwrap)
            .map(Index);

        for (i, j) in iter::zip(
            range.clone(),
            range.clone().skip(1).map(Option::Some).chain(Option::None),
        ) {
            self[i].meta.store(Owned::new(j, size::Small::default()));
        }
    }
}

impl<'raw, M> core::ops::Index<Index> for Slice<'raw, M> {
    type Output = Slab<M>;
    fn index(&self, index: Index) -> &Self::Output {
        unsafe { self.base.add(index.0.get() as usize).as_ref() }
    }
}

#[repr(C, align(64))]
pub(crate) struct Slab<M> {
    pub(crate) meta: Atomic<M>,
    pub(crate) free: block::Set<7>,
}

#[repr(C)]
pub(crate) struct Owned(u64);
// next: Option<Index>,
// class: size::Small,

impl Owned {
    pub(crate) fn new(next: Option<Index>, class: size::Small) -> Self {
        Self(next.pack() << 32 | class.pack())
    }

    pub(crate) fn next(&self) -> Option<Index> {
        Packed::unpack(self.0 >> 32)
    }

    pub(crate) fn class(&self) -> size::Small {
        Packed::unpack(self.0)
    }
}

impl Debug for Owned {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Owned")
            .field("next", &self.next())
            .field("class", &self.class())
            .finish()
    }
}

unsafe impl Packed for Owned {
    const BITS: u8 = 64;

    fn pack(&self) -> u64 {
        self.0
    }

    fn unpack(value: u64) -> Self {
        Self(value)
    }
}

#[repr(C)]
pub(crate) struct Shared(u64);
// version: Wrapping<u16>,
// class: size::Small,

unsafe impl Packed for Shared {
    const BITS: u8 = 64;

    fn pack(&self) -> u64 {
        self.0
    }

    fn unpack(value: u64) -> Self {
        Self(value)
    }
}

#[repr(C)]
pub(crate) struct LocalStack {
    head: Option<Index>,
}

impl LocalStack {
    pub(crate) fn peek(&self) -> Option<Index> {
        self.head
    }

    pub(crate) fn set(&mut self, head: Option<Index>) {
        self.head = head;
    }

    // FIXME: type of `head` for integration with `meta::Shared::allocate`
    pub(crate) unsafe fn set_raw(&mut self, head: u32) {
        let head = head
            .checked_add(1)
            .map(NonZeroU32::new)
            .unwrap()
            .map(Index)
            .unwrap();

        self.head = Some(head);
    }

    pub(crate) fn trace<'a>(&self, slabs: &'a Slice<Owned>) -> impl Iterator<Item = Index> + 'a {
        let mut head = self.head;
        iter::from_fn(move || {
            let next = head?;
            head = slabs[next].meta.load().next();
            Some(next)
        })
    }
}

#[repr(C)]
pub(crate) struct GlobalStack;
