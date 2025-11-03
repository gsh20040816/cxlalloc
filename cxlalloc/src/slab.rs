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
use core::ops::Range;
use core::ptr::NonNull;
use core::sync::atomic::Ordering;

use crate::cache;
use crate::cas::Detectable;
use crate::data;
use crate::size;
use crate::thread;

pub(crate) struct Slab<'raw, B: size::Bracket> {
    locals: Slice<'raw, B, Local<B>>,
    remotes: Slice<'raw, B, Detectable<Remote>>,
}

impl<'raw, B: size::Bracket> Slab<'raw, B> {
    pub(crate) fn new(
        locals: Slice<'raw, B, Local<B>>,
        remotes: Slice<'raw, B, Detectable<Remote>>,
    ) -> Self {
        Self { locals, remotes }
    }

    #[inline]
    pub(crate) fn local(&self, index: Index<B>) -> &Local<B> {
        &self.locals[index]
    }

    #[inline]
    pub(crate) fn remote(&self, index: Index<B>) -> &Detectable<Remote> {
        &self.remotes[index]
    }

    pub(crate) unsafe fn link(
        &self,
        id: thread::Id,
        range: Range<Index<B>>,
        head: Option<Index<B>>,
    ) {
        let range = (range.start.value.get()..range.end.value.get())
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
            let local = self.local(i);
            local.own(id);
            local.next.store(j, Ordering::Relaxed);
            cache::flush(&local.next, cache::Invalidate::No);
        }
    }

    pub(crate) fn trace(&self, mut head: Option<Index<B>>) -> impl Iterator<Item = Index<B>> + '_ {
        iter::from_fn(move || {
            let next = head?;
            head = self.local(next).next.load(Ordering::Relaxed);
            Some(next)
        })
    }
}

#[repr(transparent)]
#[derive(ribbit::Pack)]
#[ribbit(size = 32, nonzero, new(vis = ""))]
pub(crate) struct Index<B> {
    #[ribbit(size = 0)]
    _bracket: PhantomData<B>,
    value: NonZeroU32,
}

impl<B> Index<B> {
    pub(crate) const MIN: Self = Self::new(NonZeroU32::MIN);

    const fn new(value: NonZeroU32) -> Self {
        Self {
            value,
            _bracket: PhantomData,
        }
    }
}

impl<B> Copy for Index<B> {}
impl<B> Clone for Index<B> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<B> PartialEq for Index<B> {
    fn eq(&self, other: &Self) -> bool {
        self.value.eq(&other.value)
    }
}
impl<B> Eq for Index<B> {}
impl<B> core::hash::Hash for Index<B> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.value.hash(state)
    }
}

impl Index<size::Huge> {
    pub(crate) fn new_huge(slot: usize) -> Self {
        Self::new(NonZeroU32::MIN.checked_add(slot as u32).unwrap())
    }
}

impl<B> Index<B> {
    pub(crate) unsafe fn add(&self, count: u32) -> Self {
        self.value.checked_add(count).map(Self::new).unwrap()
    }
}

impl<B> Debug for Index<B> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        Debug::fmt(&(self.value.get() - 1), f)
    }
}

impl<B> Display for Index<B> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(&(self.value.get() - 1), f)
    }
}

impl<B> From<Index<B>> for NonZeroU32 {
    fn from(index: Index<B>) -> Self {
        index.value
    }
}

impl<B> From<Index<B>> for u32 {
    fn from(index: Index<B>) -> Self {
        index.value.get() - 1
    }
}

impl<B: size::Bracket> From<data::Offset<B>> for Index<B> {
    fn from(offset: data::Offset<B>) -> Self {
        let offset = NonZeroU64::from(offset);
        let index = offset.get() / B::SIZE_SLAB as u64;
        NonZeroU32::new(index as u32).map(Self::new).unwrap()
    }
}

pub(crate) struct Slice<'raw, B, T> {
    base: NonNull<T>,
    _bracket: PhantomData<B>,
    _raw: PhantomData<&'raw ()>,
}

impl<B, T> Slice<'_, B, T> {
    pub(crate) fn layout(count: usize) -> Result<Layout, LayoutError> {
        Layout::array::<T>(count)
    }

    // Implementation detail: store minus one
    pub(crate) unsafe fn from_raw(base: NonNull<T>) -> Self {
        let base = base.as_ptr().wrapping_sub(1);

        Self {
            base: NonNull::new(base).unwrap(),
            _bracket: PhantomData,
            _raw: PhantomData,
        }
    }
}

impl<B, T> core::ops::Index<Index<B>> for Slice<'_, B, T> {
    type Output = T;
    fn index(&self, index: Index<B>) -> &Self::Output {
        unsafe { self.base.add(index.value.get() as usize).as_ref() }
    }
}
