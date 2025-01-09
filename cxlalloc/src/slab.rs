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
use core::num::NonZeroU64;
use core::num::NonZeroUsize;
use core::ops::Range;
use core::ptr::NonNull;

use ribbit::private::u12;

use crate::atomic::Version;
use crate::bitset::Bit;
use crate::cas;
use crate::raw;
use crate::region;
use crate::region::owned::GlobalToLocal;
use crate::region::owned::LocalToGlobal;
use crate::region::owned::StateUnpacked;
use crate::region::shared::Length;
use crate::size;
use crate::thread;
use crate::SIZE_SLAB;

#[ribbit::pack(size = 32, nonzero, new(vis = ""))]
#[repr(transparent)]
#[derive(Copy, Clone, PartialEq, Eq)]
pub(crate) struct Index(NonZeroU32);

impl Index {
    pub(crate) fn from_length(length: Length) -> Self {
        NonZeroU32::new(u32::from(length) + 1)
            .map(Self::new)
            .unwrap()
    }

    #[inline]
    pub(crate) unsafe fn offset_block(&self, class: size::Class, index: Bit) -> Offset {
        debug_assert!(usize::from(index) <= class.count(), "{} {:?}", class, index);
        let base = NonZeroUsize::from(Offset::from(*self));
        let delta = class.size() * usize::from(index);
        NonZeroU64::try_from(base.checked_add(delta).unwrap())
            .map(Offset::new_internal)
            .unwrap()
    }

    pub(crate) unsafe fn add(&self, count: u32) -> Self {
        self._0().checked_add(count).map(Self::new).unwrap()
    }
}

impl Debug for Index {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        Debug::fmt(&(self._0().get() - 1), f)
    }
}

impl Display for Index {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(&(self._0().get() - 1), f)
    }
}

impl From<Offset> for Index {
    #[inline]
    fn from(offset: Offset) -> Self {
        unsafe {
            Self::new(NonZeroU32::new_unchecked(
                (offset._0().get() as usize / SIZE_SLAB) as u32,
            ))
        }
    }
}

impl From<Index> for NonZeroU32 {
    fn from(index: Index) -> Self {
        index._0()
    }
}

impl From<Index> for u32 {
    fn from(index: Index) -> Self {
        index._0().get() - 1
    }
}

#[ribbit::pack(size = 64, nonzero, new(rename = "new_internal", vis = ""))]
#[repr(transparent)]
#[derive(Copy, Clone, PartialEq, Eq)]
pub struct Offset(NonZeroU64);

impl Offset {
    pub(crate) unsafe fn new(offset: NonZeroUsize) -> Self {
        Self::new_internal(offset.try_into().unwrap())
    }

    pub(crate) fn get(&self) -> NonZeroUsize {
        self._0().try_into().unwrap()
    }

    #[inline]
    #[track_caller]
    pub(crate) unsafe fn index_block(&self, class: size::Class) -> Bit {
        Bit::new(u12::new(
            ((self.get().get() % SIZE_SLAB) / class.size()) as u16,
        ))
    }
}

impl From<Index> for Offset {
    #[inline]
    fn from(index: Index) -> Self {
        NonZeroUsize::new(index._0().get() as usize * SIZE_SLAB)
            .map(|offset| unsafe { Self::new(offset) })
            .unwrap()
    }
}

impl From<Offset> for NonZeroUsize {
    fn from(value: Offset) -> Self {
        value.get()
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
        value.get().get() - SIZE_SLAB
    }
}

#[repr(C, align(64))]
pub(crate) struct Descriptor {
    pub(crate) owned: Owned,
    pub(crate) shared: Shared,
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
        let range = (range.start._0().get()..range.end._0().get())
            .map(NonZeroU32::new)
            .map(Option::unwrap)
            .map(Index::new);

        for (i, j) in iter::zip(
            range.clone(),
            range
                .clone()
                .skip(1)
                .map(Option::Some)
                .chain(iter::once(head)),
        ) {
            let meta = &self[i].next;
            meta.store(j);
            crate::flush(meta, false);
        }
    }

    pub(crate) fn trace(&self, mut head: Option<Index>) -> impl Iterator<Item = Index> + '_ {
        iter::from_fn(move || {
            let next = head?;
            head = self[next].next.load();
            Some(next)
        })
    }
}

impl<S> core::ops::Index<Index> for Slice<'_, S> {
    type Output = S;
    fn index(&self, index: Index) -> &Self::Output {
        unsafe { self.base.add(index._0().get() as usize).as_ref() }
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
        crate::flush(&self, false);
    }

    pub(crate) fn pop(&mut self, slabs: &Slice<Owned>) -> Option<Index> {
        let index = self.head?;
        self.count -= 1;
        self.head = slabs[index].next.load();
        crate::flush(self, false);
        Some(index)
    }

    pub(crate) fn push(&mut self, slabs: &Slice<Owned>, index: Index) {
        let slab = &slabs[index];
        slab.next.store(self.head);
        crate::flush(&slab.next, false);

        self.count += 1;
        self.head = Some(index);
        crate::flush(&self.head, false);
    }

    pub(crate) fn trace<'a>(&self, slabs: &'a Slice<Owned>) -> impl Iterator<Item = Index> + 'a {
        slabs.trace(self.head)
    }
}

#[repr(C)]
pub(crate) struct GlobalStack {
    head: cas::Detectable<Option<Index>>,
}

impl GlobalStack {
    pub(crate) fn push(
        &self,
        id: thread::Id,
        slabs: &Slice<Owned>,
        help: &thread::Array<cas::Help>,
        head: Index,
        tail: Index,
    ) {
        self.head.update(help, id, |old, version| {
            slabs[tail].next.store(old);
            crate::flush(&slabs[tail].next, false);
            Some((
                Some(head),
                StateUnpacked::LocalToGlobal(LocalToGlobal::new(head, version)),
            ))
        });
    }

    pub(crate) fn pop(
        &self,
        id: thread::Id,
        slabs: &Slice<Owned>,
        help: &thread::Array<cas::Help>,
    ) -> Option<Index> {
        self.head
            .update(help, id, |old, version| {
                let old = old?;
                let new = slabs[old].next.load();

                Some((
                    new,
                    StateUnpacked::GlobalToLocal(GlobalToLocal::new(old, version)),
                ))
            })
            .flatten()
    }

    pub(crate) fn is_empty(&self, help: &thread::Array<cas::Help>) -> bool {
        self.head.load(help).is_none()
    }
}

#[ribbit::pack(size = 64, debug)]
#[derive(Copy, Clone, PartialEq, Eq)]
struct Head {
    #[ribbit(size = 16, nonzero)]
    id: thread::Id,

    #[ribbit(size = 16)]
    version: Version,

    #[ribbit(size = 32)]
    index: Option<Index>,
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
