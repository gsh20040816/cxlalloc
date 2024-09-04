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
use crate::Atomic;
use crate::BitSet;
use crate::Transfer;
use crate::SIZE_SLAB;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub(crate) struct Index(NonZeroU32);

impl Index {
    pub(crate) fn from_length(length: u32) -> Self {
        NonZeroU32::new(length + 1).map(Self).unwrap()
    }

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

impl From<Offset> for Index {
    fn from(offset: Offset) -> Self {
        u32::try_from(offset.0.get() / SIZE_SLAB)
            .map(NonZeroU32::new)
            .unwrap()
            .map(Self)
            .unwrap()
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

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct Offset(NonZeroUsize);

impl Offset {
    pub(crate) unsafe fn new(delta: NonZeroUsize) -> Self {
        Self(delta)
    }

    pub(crate) unsafe fn index_block(&self, class: size::Small) -> Bit {
        Bit::new((self.0.get() % SIZE_SLAB) / class.size())
    }
}

impl From<Index> for Offset {
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
                .store(Owned::new(j, size::Class::Small(size::Small::default())));
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
    pub(crate) free: BitSet<7>,
}

#[repr(C)]
pub(crate) struct Owned(u64);
// next: Option<Index>,
// class: size::Class,

impl Owned {
    pub(crate) fn new(next: Option<Index>, class: size::Class) -> Self {
        Self(next.pack() << 32 | class.pack())
    }

    pub(crate) fn next(&self) -> Option<Index> {
        Packed::unpack(self.0 >> 32)
    }

    pub(crate) fn class(&self) -> size::Class {
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

    pub(crate) fn pop(&mut self, slabs: &Slice<Owned>) {
        let Some(index) = self.head else {
            return;
        };

        self.head = slabs[index].meta.load().next();
    }

    pub(crate) fn push(&mut self, slabs: &Slice<Owned>, index: Index, class: Option<size::Small>) {
        slabs[index].meta.store(Owned::new(
            self.head,
            size::Class::Small(class.unwrap_or_default()),
        ));
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
