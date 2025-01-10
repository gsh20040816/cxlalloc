use core::cell::UnsafeCell;
use core::fmt::Display;
use core::ops::Add;

use ribbit::private::u24;

use crate::cas;
use crate::crash;
use crate::size;
use crate::slab;
use crate::thread;
use crate::view;
use crate::Epoch;

pub struct Heap<'raw, B> {
    /// Multiple-reader, multiple-writer metadata
    pub(crate) shared: &'raw Shared,

    /// Single-reader, single-writer metadata
    pub(crate) owned: &'raw thread::Array<UnsafeCell<Owned>>,

    pub(crate) slabs: view::Slab<'raw, B>,
    pub(crate) data: view::Data<'raw, B>,
}

impl<'raw, B> Heap<'raw, B> {
    pub(crate) fn new(
        shared: &'raw Shared,
        owned: &'raw thread::Array<UnsafeCell<Owned>>,
        slabs: view::Slab<'raw, B>,
        data: view::Data<'raw, B>,
    ) -> Self {
        Self {
            shared,
            owned,
            slabs,
            data,
        }
    }
}

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

pub(crate) struct Owned {
    pub(crate) r#unsized: slab::LocalStack,
    pub(crate) r#sized: size::Array<slab::LocalStack>,
}

impl Owned {
    // #[inline]
    // pub(crate) fn log_sync(&mut self, state: StateUnpacked) {
    //     if !cfg!(feature = "recover-log") {
    //         return;
    //     }
    //
    //     crate::fence();
    //     self.log_unsync(State::new(state));
    //     crate::fence();
    // }
    //
    // #[inline]
    // pub(crate) fn log_unsync(&mut self, state: State) {
    //     if !cfg!(feature = "recover-log") {
    //         return;
    //     }
    //
    //     self.state.store(Some(state));
    //     crate::flush(&self.state, false);
    // }

    pub(crate) fn unsized_to_sized(
        &mut self,
        owned: &slab::Slice<slab::Owned>,
        shared: &slab::Slice<slab::Shared>,
        id: thread::Id,
        class: size::Small,
    ) -> bool {
        let Some(index) = self.r#unsized.peek() else {
            return false;
        };

        crash::define!(unsized_to_sized_pre_log);

        let slab = &owned[index];
        let next = slab.next.load();

        // self.log_sync(StateUnpacked::UnsizedToSized(UnsizedToSized::new(
        //     next, class,
        // )));

        self.r#sized[class].push(owned, index);
        unsafe {
            (*slab.free.get()).fill(class.count());
        }

        shared[index]
            .owner
            .store(slab::shared::Owner::new(class, Some(id)));
        crate::flush(&shared[index].owner, false);

        let count = self.r#unsized.len();
        self.r#unsized.set(next, count - 1);
        true
    }

    #[cold]
    pub(crate) fn sized_to_unsized(
        &mut self,
        slabs: &slab::Slice<slab::Owned>,
        class: size::Small,
        index: slab::Index,
    ) {
        // Special case: not in sized list
        if class == size::SLAB {
            return self.r#unsized.push(slabs, index);
        }

        let next = slabs[index].next.load();

        let mut walk = self.r#sized[class].peek().unwrap();

        if walk == index {
            let count = self.r#sized[class].len();
            self.r#sized[class].set(next, count - 1);
        } else {
            let prev = loop {
                match slabs[walk].next.load() {
                    None => panic!("removing non-existent slab {} {}", index, class),
                    Some(next) if next == index => break walk,
                    Some(next) => walk = next,
                }
            };

            slabs[prev].next.store(next);
            crate::flush(&slabs[prev], false);
        };

        self.r#unsized.push(slabs, index);
    }
}
