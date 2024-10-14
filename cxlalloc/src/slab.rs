pub(crate) mod owned;
pub(crate) mod shared;

pub(crate) use owned::Owned;
pub(crate) use shared::Shared;

use core::alloc::Layout;
use core::alloc::LayoutError;
use core::fmt;
use core::fmt::Debug;
use core::fmt::Display;
use core::iter;
use core::marker::PhantomData;
use core::num::NonZeroU32;
use core::num::NonZeroUsize;
use core::ops::Range;
use core::ptr::NonNull;

use crate::atomic::NonZero;
use crate::atomic::Packed;
use crate::atomic::Version;
use crate::bitset::Bit;
use crate::raw;
use crate::region::shared::Help;
use crate::region::shared::Length;
use crate::size;
use crate::thread;
use crate::Atomic;
use crate::SIZE_SLAB;

#[repr(C)]
#[derive(Copy, Clone, PartialEq, Eq)]
pub(crate) struct Index(NonZeroU32);

impl Index {
    pub(crate) fn from_length(length: Length) -> Self {
        NonZeroU32::new(u32::from(length) + 1).map(Self).unwrap()
    }

    #[inline]
    pub(crate) unsafe fn offset_block(&self, class: size::Class, index: Bit) -> Offset {
        debug_assert!(usize::from(index) <= class.count(), "{} {:?}", class, index);
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
        Debug::fmt(&(self.0.get() - 1), f)
    }
}

impl Display for Index {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(&(self.0.get() - 1), f)
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

impl From<Index> for u32 {
    fn from(index: Index) -> Self {
        index.0.get() - 1
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
    #[track_caller]
    pub(crate) unsafe fn index_block(&self, class: size::Class) -> Bit {
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
        Debug::fmt(&usize::from(*self), f)
    }
}

impl Display for Offset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(&usize::from(*self), f)
    }
}

impl From<Offset> for usize {
    fn from(value: Offset) -> Self {
        value.0.get() - SIZE_SLAB
    }
}

unsafe impl Packed for Offset {
    const BITS: u8 = 48;

    fn pack(&self) -> u64 {
        self.0.get() as u64
    }

    fn unpack(value: u64) -> Self {
        unsafe { Self(NonZeroUsize::new_unchecked((value & Self::MASK) as usize)) }
    }
}

unsafe impl NonZero for Offset {}

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

pub(crate) trait Slab: private::Seal {}

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
            self[i].meta.store(owned::Meta::new(j));
        }
    }

    pub(crate) fn trace(&self, mut head: Option<Index>) -> impl Iterator<Item = Index> + '_ {
        iter::from_fn(move || {
            let next = head?;
            head = self[next].meta.load().next();
            Some(next)
        })
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
    count: usize,
}

impl LocalStack {
    pub(crate) fn peek(&self) -> Option<Index> {
        self.head
    }

    pub(crate) fn len(&self) -> usize {
        self.count
    }

    pub(crate) fn set(&mut self, head: Option<Index>, count: usize) {
        self.count = count;
        self.head = head;
    }

    pub(crate) fn pop(&mut self, slabs: &Slice<Owned>) -> Option<Index> {
        let index = self.head?;
        self.count -= 1;
        self.head = slabs[index].meta.load().next();
        Some(index)
    }

    pub(crate) fn push(&mut self, slabs: &Slice<Owned>, index: Index) {
        slabs[index].meta.store(owned::Meta::new(self.head));
        self.count += 1;
        self.head = Some(index);
    }

    pub(crate) fn trace<'a>(&self, slabs: &'a Slice<Owned>) -> impl Iterator<Item = Index> + 'a {
        slabs.trace(self.head)
    }
}

#[repr(C)]
pub(crate) struct GlobalStack<'raw> {
    head: Atomic<Head>,
    _raw: PhantomData<&'raw raw::Heap>,
}

impl<'raw> GlobalStack<'raw> {
    pub(crate) fn push(
        &self,
        id: thread::Id,
        slabs: &Slice<Owned>,
        helps: &thread::Array<Help>,
        head: Index,
        tail: Index,
    ) {
        let mut old = self.head.load();
        let version = helps[id].peek().next();
        helps[id].prepare(version);

        loop {
            if let Some(id) = old.id() {
                helps[id].notify(old.version());
            }

            slabs[tail].meta.store(owned::Meta::new(old.index()));
            match self
                .head
                .compare_exchange(old, Head::new(id, version, Some(head)))
            {
                Ok(_) => break,
                Err(next) => old = next,
            }
        }
    }

    pub(crate) fn pop(
        &self,
        id: thread::Id,
        slabs: &Slice<Owned>,
        helps: &thread::Array<Help>,
    ) -> Option<Index> {
        let mut old = self.head.load();
        let version = helps[id].peek().next();
        helps[id].prepare(version);

        loop {
            if let Some(id) = old.id() {
                helps[id].notify(old.version());
            }

            let index = old.index()?;
            let new = Head::new(id, version, slabs[index].meta.load().next());

            match self.head.compare_exchange(old, new) {
                Ok(_) => break Some(index),
                Err(next) => old = next,
            }
        }
    }
}

impl<'raw> GlobalStack<'raw> {
    pub(crate) fn is_empty(&self) -> bool {
        self.head.load().index().is_none()
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
struct Head(u64);

impl Head {
    fn new(id: thread::Id, version: Version, index: Option<Index>) -> Self {
        Self((id.pack() << 48) | (version.pack() << 32) | index.pack())
    }

    fn index(&self) -> Option<Index> {
        Packed::unpack(self.0 as u32 as u64)
    }

    fn version(&self) -> Version {
        Packed::unpack((self.0 >> 32) as u16 as u64)
    }

    fn id(&self) -> Option<thread::Id> {
        Packed::unpack(self.0 >> 48)
    }
}

impl Debug for Head {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("")
            .field("id", &self.id())
            .field("version", &self.version())
            .field("index", &self.index())
            .finish()
    }
}

unsafe impl Packed for Head {
    const BITS: u8 = 64;

    fn pack(&self) -> u64 {
        self.0
    }

    fn unpack(value: u64) -> Self {
        Self(value)
    }
}

#[inline]
pub(crate) fn transfer(
    shared: &Slice<Shared>,
    owned: &Slice<Owned>,
    index: Index,
    old: Option<thread::Id>,
    new: Option<thread::Id>,
) {
    if !cfg!(feature = "validate") {
        return;
    }

    let Err(actual) = owned[index].owner.transfer(old, new) else {
        return;
    };

    let meta = shared[index].meta.load();
    let owner = shared[index].owner.load();

    panic!(
        "Slab {index} transfer failed: \
        old = {old:?}, \
        new = {new:?}, \
        actual = {actual:?}, \
        version = {:?}, \
        claim = {:?}, \
        class = {}, \
        owner = {:?}, \
        owned = {:?}, \
        shared = {:?}",
        meta.version(),
        meta.claim(),
        owner.class(),
        owner.id(),
        unsafe { &*owned[index].free.get() },
        &shared[index].free,
    );
}

#[inline]
pub(crate) fn transfer_all(
    shared: &Slice<Shared>,
    owned: &Slice<Owned>,
    index: Index,
    count: usize,
    old: Option<thread::Id>,
    new: Option<thread::Id>,
) {
    if !cfg!(feature = "validate") {
        return;
    }

    for i in 0..count {
        transfer(shared, owned, unsafe { index.add(i as u32) }, old, new);
    }
}
