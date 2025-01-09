use core::alloc::Layout;
use core::fmt::Display;
use core::ops::Add;
use core::ops::Index;
use core::ops::Range;

use ribbit::private::u24;

use crate::cas;
use crate::extend::Epoch;
use crate::huge;
use crate::raw;
use crate::raw::Backend;
use crate::root;
use crate::slab;
use crate::thread;
use crate::view;
use crate::view::owned::BumpToLocal;
use crate::view::owned::StateUnpacked;
use crate::Atomic;
use crate::BATCH_BUMP_POP;
use crate::SIZE_PAGE;

#[repr(C)]
pub(crate) struct Shared {
    free: slab::GlobalStack,
    bump: cas::Detectable<Bump>,
}

impl Shared {
    // pub(crate) fn layout(slab_count: usize) -> Layout {
    //     Layout::new::<Meta>()
    //         .extend(slab::Slice::<slab::Shared>::layout(slab_count).unwrap())
    //         .unwrap()
    //         .0
    //         .align_to(SIZE_PAGE)
    //         .unwrap()
    //         .pad_to_align()
    // }
    //
    // pub(crate) unsafe fn from_raw(raw: &'raw raw::heap::Heap) -> Self {
    //     // FIXME: deduplicate with `layout`
    //     let offset = Layout::new::<Meta>()
    //         .extend(Layout::array::<slab::Shared>(1).unwrap())
    //         .unwrap()
    //         .1;
    //
    //     Self {
    //         capacity: raw.capacity,
    //         process_count: raw.process_count,
    //         process_id: raw.process_id,
    //         backend: &raw.backend,
    //         meta: raw.shared.base().cast::<Meta>().as_ref(),
    //         slabs: slab::Slice::from_raw(&raw.shared, offset),
    //     }
    // }
    //
    // pub(crate) fn bump(
    //     &self,
    //     id: thread::Id,
    //     meta: &mut region::Owned,
    // ) -> Option<Range<slab::Index>> {
    //     let bump = self
    //         .meta
    //         .bump
    //         .update(&self.meta.help, id, meta, |old, version| {
    //             let old_len = old.length();
    //             let new_len = old_len + BATCH_BUMP_POP;
    //
    //             if u32::from(new_len) >= old.epoch().total(self.capacity) {
    //                 panic!(
    //                     "Heap extension not yet enabled. Tried to expand from {:#x} to {:#x} but capacity is {:#x}.",
    //                     u32::from(old_len),
    //                     u32::from(new_len),
    //                     self.capacity
    //                 );
    //             } else {
    //                 Some((
    //                     old.with_length(new_len),
    //                     StateUnpacked::BumpToLocal(BumpToLocal::new(old, version)),
    //                 ))
    //             }
    //         })?;
    //
    //     let start = slab::Index::from_length(bump.length());
    //     let end = slab::Index::from_length(bump.length() + BATCH_BUMP_POP);
    //     Some(start..end)
    // }
    //
    // pub(crate) fn push(
    //     &self,
    //     id: thread::Id,
    //     // meta: &mut region::owned::Meta,
    //     slabs: &slab::Slice<slab::Owned>,
    //     head: slab::Index,
    //     tail: slab::Index,
    // ) {
    //     self.free.push(id, meta, slabs, &self.meta.help, head, tail);
    // }
    //
    // pub(crate) fn pop(
    //     &self,
    //     id: thread::Id,
    //     meta: &mut region::owned::Meta,
    //     slabs: &slab::Slice<slab::Owned>,
    // ) -> Option<slab::Index> {
    //     if self.free.is_empty(&self.meta.help) {
    //         return None;
    //     }
    //
    //     self.free.pop(id, meta, slabs, &self.meta.help)
    // }
}

#[ribbit::pack(size = 32, debug, new(vis = ""))]
#[derive(Copy, Clone)]
pub(crate) struct Bump {
    #[ribbit(size = 24)]
    length: Length,
    #[ribbit(size = 8)]
    epoch: Epoch,
}

#[ribbit::pack(size = 24)]
#[derive(Copy, Clone)]
pub(crate) struct Length(u24);

impl From<Length> for u32 {
    fn from(length: Length) -> Self {
        length._0().value()
    }
}

impl Add<u32> for Length {
    type Output = Self;
    fn add(self, rhs: u32) -> Self::Output {
        Self::new(self._0() + u24::new(rhs))
    }
}

impl Display for Length {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        Display::fmt(&u32::from(*self), f)
    }
}
