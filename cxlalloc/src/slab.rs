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
use core::num::NonZeroU64;
use core::ops::Deref;
use core::ops::Range;
use core::ptr::NonNull;

use crate::data;
use crate::raw;
use crate::size;
use crate::thread;
use crate::view::heap::Length;

pub(crate) struct Slab<'raw, B> {
    descriptors: Slice<'raw, B>,
    _raw: PhantomData<&'raw raw::Region>,
}

impl<'raw, B> Slab<'raw, B> {
    pub(crate) fn new(descriptors: Slice<'raw, B>) -> Self {
        Self {
            descriptors,
            _raw: PhantomData,
        }
    }

    pub(crate) fn layout(count: usize) -> Result<Layout, LayoutError> {
        Slice::<B>::layout(count)
    }
}

impl<'raw, B> Deref for Slab<'raw, B> {
    type Target = Slice<'raw, B>;
    fn deref(&self) -> &Self::Target {
        &self.descriptors
    }
}

#[ribbit::pack(size = 32, nonzero, new(vis = ""))]
#[repr(transparent)]
pub(crate) struct Index<B> {
    value: NonZeroU32,
    #[ribbit(size = 0)]
    _bracket: B,
}

impl Index<size::Huge> {
    pub(crate) fn new_huge(slot: usize) -> Self {
        Self::new(NonZeroU32::MIN.checked_add(slot as u32).unwrap())
    }
}

impl<B> Clone for Index<B> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<B> Copy for Index<B> {}

impl<B> Eq for Index<B> {}

impl<B> PartialEq for Index<B> {
    fn eq(&self, other: &Self) -> bool {
        self.value() == other.value()
    }
}

impl<B> Index<B> {
    pub(crate) fn from_length(length: Length) -> Self {
        NonZeroU32::new(u32::from(length) + 1)
            .map(Self::new)
            .unwrap()
    }

    pub(crate) unsafe fn add(&self, count: u32) -> Self {
        self.value().checked_add(count).map(Self::new).unwrap()
    }
}

impl<B> Debug for Index<B> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        Debug::fmt(&(self.value().get() - 1), f)
    }
}

impl<B> Display for Index<B> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(&(self.value().get() - 1), f)
    }
}

impl<B> From<Index<B>> for NonZeroU32 {
    fn from(index: Index<B>) -> Self {
        index.value()
    }
}

impl<B> From<Index<B>> for u32 {
    fn from(index: Index<B>) -> Self {
        index.value().get() - 1
    }
}

impl<B: size::Bracket> From<data::Offset<B>> for Index<B> {
    fn from(offset: data::Offset<B>) -> Self {
        let offset = NonZeroU64::from(offset);
        let index = offset.get() / B::SIZE_SLAB as u64;
        NonZeroU32::new(index as u32).map(Self::new).unwrap()
    }
}

#[repr(C, align(64))]
pub(crate) struct Descriptor<B> {
    pub(crate) local: Local<B>,
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

    pub(crate) unsafe fn link(&self, range: Range<Index<B>>, head: Option<Index<B>>) {
        let range = (range.start.value().get()..range.end.value().get())
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

    pub(crate) fn trace(&self, mut head: Option<Index<B>>) -> impl Iterator<Item = Index<B>> + '_ {
        iter::from_fn(move || {
            let next = head?;
            head = self[next].local.next.load();
            Some(next)
        })
    }
}

impl<B> core::ops::Index<Index<B>> for Slice<'_, B> {
    type Output = Descriptor<B>;
    fn index(&self, index: Index<B>) -> &Self::Output {
        unsafe { self.base.add(index.value().get() as usize).as_ref() }
    }
}

#[inline]
pub(crate) fn transfer<B>(
    slabs: &Slice<B>,
    index: Index<B>,
    old: Option<thread::Id>,
    new: Option<thread::Id>,
) where
    B: Display + ribbit::Pack<Loose = u8>,
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
    index: Index<B>,
    count: usize,
    old: Option<thread::Id>,
    new: Option<thread::Id>,
) where
    B: Display + ribbit::Pack<Loose = u8>,
{
    if !cfg!(feature = "validate") {
        return;
    }

    for i in 0..count {
        transfer(slabs, unsafe { index.add(i as u32) }, old, new);
    }
}
