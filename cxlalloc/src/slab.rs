pub(crate) mod local;
pub(crate) mod remote;
pub(crate) mod stack;

pub(crate) use local::Local;
pub(crate) use remote::Remote;

use core::alloc::Layout;
use core::alloc::LayoutError;
use core::fmt;
use core::fmt::Debug;
use core::fmt::Display;
use core::iter;
use core::marker::PhantomData;
use core::num::NonZeroU32;
use core::ops::Range;
use core::ptr::NonNull;

use crate::bitset::Bit;
use crate::raw;
use crate::size;
use crate::size::Bracket as _;
use crate::thread;
use crate::view::heap::Length;

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
    pub(crate) unsafe fn offset(&self, class: size::Small, index: Bit) -> usize {
        debug_assert!(usize::from(index) <= class.count(), "{} {:?}", class, index);
        todo!()
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

#[repr(C, align(64))]
pub(crate) struct Descriptor<B> {
    pub(crate) local: Local,
    pub(crate) remote: Remote<B>,
}

pub(crate) struct Slice<'raw, B> {
    base: NonNull<Descriptor<B>>,
    _raw: PhantomData<&'raw raw::Region>,
}

impl<B> Slice<'_, B> {
    pub(crate) fn layout(count: usize) -> Result<Layout, LayoutError> {
        Layout::array::<Descriptor<B>>(count)
    }

    // Implementation detail: store minus one
    pub(crate) unsafe fn from_raw(base: NonNull<Descriptor<B>>) -> Self {
        let base = base.as_ptr().wrapping_sub(1);

        Self {
            base: NonNull::new(base).unwrap(),
            _raw: PhantomData,
        }
    }

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
            let next = &self[i].local.next;
            next.store(j);
            crate::flush(next, false);
        }
    }

    pub(crate) fn trace(&self, mut head: Option<Index>) -> impl Iterator<Item = Index> + '_ {
        iter::from_fn(move || {
            let next = head?;
            head = self[next].local.next.load();
            Some(next)
        })
    }
}

impl<B> core::ops::Index<Index> for Slice<'_, B> {
    type Output = Descriptor<B>;
    fn index(&self, index: Index) -> &Self::Output {
        unsafe { self.base.add(index._0().get() as usize).as_ref() }
    }
}

#[inline]
pub(crate) fn transfer<B>(
    slabs: &Slice<B>,
    index: Index,
    old: Option<thread::Id>,
    new: Option<thread::Id>,
) where
    B: size::Bracket,
{
    if !cfg!(feature = "validate") {
        return;
    }

    let slab = &slabs[index];

    let Err(actual) = slab.local.owner.transfer(old, new) else {
        return;
    };

    let meta = slab.remote.meta.load();
    let owner = slab.remote.owner.load();

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
        unsafe { &*slab.local.free.get() },
        &slab.remote.free,
    );
}

#[inline]
pub(crate) fn transfer_all<B>(
    slabs: &Slice<B>,
    index: Index,
    count: usize,
    old: Option<thread::Id>,
    new: Option<thread::Id>,
) where
    B: size::Bracket,
{
    if !cfg!(feature = "validate") {
        return;
    }

    for i in 0..count {
        transfer(slabs, unsafe { index.add(i as u32) }, old, new);
    }
}
