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

pub struct Heap<'raw, L: view::Lens, B> {
    /// Multiple-reader, multiple-writer metadata
    pub(crate) shared: &'raw Shared<B>,

    /// Single-reader, single-writer metadata
    pub(crate) owned: L::Scope<'raw, Owned<B>>,

    pub(crate) slabs: view::Slab<'raw, B>,
    pub(crate) data: view::Data<'raw, B>,
}

impl<'raw, L: view::Lens, B> Heap<'raw, L, B> {
    pub(crate) fn new(
        shared: &'raw Shared<B>,
        owned: L::Scope<'raw, Owned<B>>,
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

    pub(crate) unsafe fn focus(self, id: thread::Id) -> Heap<'raw, view::Focus, B> {
        Heap {
            shared: self.shared,
            owned: L::focus(self.owned, id),
            slabs: self.slabs,
            data: self.data,
        }
    }
}

#[repr(C)]
pub(crate) struct Shared<B> {
    free: slab::stack::Global<B>,
    bump: cas::Detectable<Bump>,
}

impl<B> Shared<B> {
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

pub(crate) struct Owned<B> {
    pub(crate) r#unsized: slab::stack::Local<B>,
    pub(crate) r#sized: size::Array<B, slab::stack::Local<B>>,
}

impl<B> Owned<B>
where
    B: size::Bracket,
{
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
        slabs: &slab::Slice<B>,
        id: thread::Id,
        class: B,
    ) -> bool {
        let Some(index) = self.r#unsized.peek() else {
            return false;
        };

        crash::define!(unsized_to_sized_pre_log);

        let slab = &slabs[index];
        let next = slab.local.next.load();

        // self.log_sync(StateUnpacked::UnsizedToSized(UnsizedToSized::new(
        //     next, class,
        // )));

        self.r#sized[class].push(slabs, index);
        unsafe {
            (*slab.local.free.get()).fill(class.count());
        }

        slab.remote
            .owner
            .store(slab::remote::Owner::new(class, Some(id)));
        crate::flush(&slab.remote.owner, false);

        let count = self.r#unsized.len();
        self.r#unsized.set(next, count - 1);
        true
    }

    #[cold]
    pub(crate) fn sized_to_unsized(
        &mut self,
        slabs: &slab::Slice<B>,
        class: B,
        index: slab::Index,
    ) {
        // Special case: not in sized list
        if class.is_max() {
            return self.r#unsized.push(slabs, index);
        }

        let next = slabs[index].local.next.load();

        let mut walk = self.r#sized[class].peek().unwrap();

        if walk == index {
            let count = self.r#sized[class].len();
            self.r#sized[class].set(next, count - 1);
        } else {
            let prev = loop {
                match slabs[walk].local.next.load() {
                    None => panic!("removing non-existent slab {} {}", index, class),
                    Some(next) if next == index => break walk,
                    Some(next) => walk = next,
                }
            };

            slabs[prev].local.next.store(next);
            crate::flush(&slabs[prev], false);
        };

        self.r#unsized.push(slabs, index);
    }
}
