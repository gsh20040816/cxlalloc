pub(crate) mod owned;
pub(crate) mod shared;

pub(crate) use owned::Owned;
pub(crate) use shared::Shared;

use core::alloc::Layout;
use core::alloc::LayoutError;
use core::fmt;
use core::fmt::Debug;
use core::iter;
use core::marker::PhantomData;
use core::num::NonZeroU32;
use core::num::NonZeroUsize;
use core::ops::Range;
use core::ptr::NonNull;

use crate::atomic::NonZero;
use crate::atomic::Packed;
use crate::bitset::Bit;
use crate::raw;
use crate::size;
use crate::transfer;
use crate::Transfer;
use crate::SIZE_SLAB;

#[repr(C)]
#[derive(Copy, Clone, PartialEq, Eq)]
pub(crate) struct Index(NonZeroU32);

impl Index {
    pub(crate) fn from_length(length: u32) -> Self {
        NonZeroU32::new(length + 1).map(Self).unwrap()
    }

    #[inline]
    pub(crate) unsafe fn offset_block(&self, class: size::Small, index: Bit) -> Offset {
        debug_assert!(usize::from(index) <= class.count());
        let base = NonZeroUsize::from(Offset::from(*self));
        let delta = class.size() * usize::from(index);
        base.checked_add(delta).map(Offset).unwrap()
    }

    pub(crate) unsafe fn add(&self, count: u32) -> Self {
        self.0.checked_add(count).map(Self).unwrap()
    }
}

impl Debug for Index {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        (self.0.get() - 1).fmt(f)
    }
}

impl From<Offset> for Index {
    #[inline]
    fn from(offset: Offset) -> Self {
        unsafe {
            Self(NonZeroU32::new_unchecked(
                (offset.0.get() / SIZE_SLAB) as u32,
            ))
        }
    }
}

impl From<Index> for NonZeroU32 {
    fn from(index: Index) -> Self {
        index.0
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

#[repr(C)]
#[derive(Copy, Clone, PartialEq, Eq)]
pub struct Offset(NonZeroUsize);

impl Offset {
    pub(crate) unsafe fn new(delta: NonZeroUsize) -> Self {
        Self(delta)
    }

    #[inline]
    pub(crate) unsafe fn index_block(&self, class: size::Small) -> Bit {
        Bit::new((self.0.get() % SIZE_SLAB) / class.size())
    }
}

impl From<Index> for Offset {
    #[inline]
    fn from(index: Index) -> Self {
        NonZeroUsize::new(index.0.get() as usize * SIZE_SLAB)
            .map(Self)
            .unwrap()
    }
}

impl From<Offset> for NonZeroUsize {
    fn from(value: Offset) -> Self {
        value.0
    }
}

impl Debug for Offset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (self.0.get() - SIZE_SLAB).fmt(f)
    }
}

pub(crate) struct Slice<'raw, S> {
    base: NonNull<S>,
    _raw: PhantomData<&'raw raw::Region>,
}

impl<S: Slab> Slice<'_, S> {
    pub(crate) fn layout(count: usize) -> Result<Layout, LayoutError> {
        Layout::array::<S>(count)
    }

    // Implementation detail: store minus one
    pub(crate) unsafe fn from_raw(region: &raw::Region, offset: usize) -> Self {
        let base = region
            .base()
            .byte_add(offset)
            .as_ptr()
            .cast::<S>()
            .wrapping_sub(1);

        Self {
            base: NonNull::new(base).unwrap(),
            _raw: PhantomData,
        }
    }
}

trait Slab: private::Seal {}

impl private::Seal for Owned {}
impl Slab for Owned {}

impl private::Seal for Shared {}
impl Slab for Shared {}

mod private {
    pub trait Seal {}
}

impl Slice<'_, Owned> {
    pub(crate) unsafe fn link(&self, range: Range<Index>, head: Option<Index>) {
        let range = (range.start.0.get()..range.end.0.get())
            .map(NonZeroU32::new)
            .map(Option::unwrap)
            .map(Index);

        for (i, j) in iter::zip(
            range.clone(),
            range
                .clone()
                .skip(1)
                .map(Option::Some)
                .chain(iter::once(head)),
        ) {
            self[i]
                .meta
                .store(owned::Meta::new(j, size::Small::default()));
        }
    }
}

impl<'raw, S> core::ops::Index<Index> for Slice<'raw, S> {
    type Output = S;
    fn index(&self, index: Index) -> &Self::Output {
        unsafe { self.base.add(index.0.get() as usize).as_ref() }
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

    pub(crate) fn pop(&mut self, slabs: &Slice<Owned>) -> Option<Index> {
        let index = self.head?;
        self.head = slabs[index].meta.load().next();
        Some(index)
    }

    pub(crate) fn push(&mut self, slabs: &Slice<Owned>, index: Index, class: Option<size::Small>) {
        slabs[index]
            .meta
            .store(owned::Meta::new(self.head, class.unwrap_or_default()));
        self.set(Some(index));
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
pub(crate) struct GlobalStack<'raw> {
    claim: transfer::Claim<Pop, Push>,
    head: transfer::State<Option<Index>>,
    _raw: PhantomData<&'raw raw::Heap>,
}

impl<'raw> GlobalStack<'raw> {
    pub(crate) fn is_empty(&self) -> bool {
        self.head.load().is_none()
    }
}

#[derive(Debug)]
pub struct Empty;

impl<'raw> Transfer for GlobalStack<'raw> {
    type State = Option<Index>;
    type Context = Slice<'raw, Owned>;

    type Write = Push;
    type Input = Index;

    type Read = Pop;
    type Output = Index;

    type Abort = Empty;

    fn try_read(
        &self,
        _: &Self::Context,
        Pop: Self::Read,
        head: Self::State,
    ) -> Result<Self::Output, Self::Abort> {
        match head {
            Some(index) => Ok(index),
            None => Err(Empty),
        }
    }

    fn finish_read(
        &self,
        slabs: &Self::Context,
        Pop: Self::Read,
        head: Self::State,
    ) -> Self::State {
        let index = head.expect("Non-empty head");
        slabs[index].meta.load().next()
    }

    fn interpose_write(
        &self,
        slabs: &Self::Context,
        Push(count): Self::Write,
        head: Self::State,
        index: &Self::Input,
    ) {
        // FIXME: this only needs to be done once, but `interpose_write` is called
        // every time there is a conflicting operation and we need to restart.
        // One solution would be to pass another parameter indicating whether
        // this is the first time?
        unsafe {
            slabs.link(*index..index.add(u32::from(count)), head);
        }
    }

    fn finish_write(
        &self,
        _: &Self::Context,
        Push(_): Self::Write,
        index: Self::Input,
    ) -> Self::State {
        Some(index)
    }

    fn claim(&self) -> &transfer::Claim<Self::Read, Self::Write> {
        &self.claim
    }

    fn state(&self) -> &transfer::State<Self::State> {
        &self.head
    }
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct Pop;

unsafe impl Packed for Pop {
    const BITS: u8 = 1;

    fn pack(&self) -> u64 {
        0
    }

    fn unpack(_: u64) -> Self {
        Self
    }
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct Push(u16);

impl Push {
    pub(crate) fn new(count: u16) -> Self {
        assert!(count as u64 <= <Self as Packed>::MASK);
        Self(count)
    }
}

unsafe impl Packed for Push {
    const BITS: u8 = 15;

    fn pack(&self) -> u64 {
        self.0 as u64
    }

    fn unpack(value: u64) -> Self {
        Push((value & Self::MASK) as u16)
    }
}
