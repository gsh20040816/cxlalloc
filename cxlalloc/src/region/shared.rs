use core::alloc::Layout;
use core::ffi;
use core::ops::Index;
use core::ops::Range;
use core::ptr::NonNull;
use std::sync::Mutex;

use crate::atomic::Packed;
use crate::atomic::Version;
use crate::cas;
use crate::extend::Epoch;
use crate::huge;
use crate::raw;
use crate::region;
use crate::root;
use crate::slab;
use crate::thread;
use crate::Atomic;
use crate::Barrier;
use crate::BATCH_BUMP_POP;
use crate::SIZE_PAGE;

use super::owned::State;

pub(crate) struct Shared<'raw> {
    capacity: u32,
    process_count: usize,
    process_id: usize,
    meta: &'raw Meta<'raw>,
    pub(crate) slabs: slab::Slice<'raw, slab::Shared>,
}

impl<'raw> Shared<'raw> {
    pub(crate) fn layout(slab_count: usize) -> Layout {
        Layout::new::<Meta>()
            .extend(slab::Slice::<slab::Shared>::layout(slab_count).unwrap())
            .unwrap()
            .0
            .align_to(SIZE_PAGE)
            .unwrap()
            .pad_to_align()
    }

    pub(crate) unsafe fn from_raw(raw: &'raw raw::heap::Inner) -> Self {
        // FIXME: deduplicate with `layout`
        let offset = Layout::new::<Meta>()
            .extend(Layout::array::<slab::Shared>(1).unwrap())
            .unwrap()
            .1;

        Self {
            capacity: raw.capacity,
            process_count: raw.process_count,
            process_id: raw.process_id,
            meta: raw.shared.base().cast::<Meta>().as_ref(),
            slabs: slab::Slice::from_raw(&raw.shared, offset),
        }
    }

    pub(crate) fn extend(
        &self,
        _id: thread::Id,
        _epoch: Epoch,
        _version: Option<Version>,
    ) -> Result<(), Epoch> {
        todo!()
    }

    pub(crate) fn bump(
        &self,
        id: thread::Id,
        meta: &mut region::owned::Meta,
    ) -> Option<Range<slab::Index>> {
        let length = self
            .meta
            .bump
            .update(&self.meta.help, id, meta, |old, version| {
                if old.0 + BATCH_BUMP_POP >= self.capacity {
                    None
                } else {
                    Some((
                        Length(old.0 + BATCH_BUMP_POP),
                        State::BumpToLocal {
                            length: old,
                            version,
                        },
                    ))
                }
            })?;

        let start = slab::Index::from_length(Length(length.0));
        let end = slab::Index::from_length(Length(length.0 + BATCH_BUMP_POP));
        Some(start..end)
    }

    pub(crate) fn allocate_log(
        &self,
        state: &Mutex<huge::Dram>,
        base: NonNull<u64>,
        size: usize,
    ) -> NonNull<u64> {
        self.meta
            .log
            .allocate(state, self.process_count, self.process_id, base, size)
    }

    pub(crate) unsafe fn free_log(
        &self,
        state: &Mutex<huge::Dram>,
        base: NonNull<u64>,
        pointer: NonNull<ffi::c_void>,
    ) {
        self.meta
            .log
            .free(state, self.process_count, self.process_id, base, pointer)
    }

    pub(crate) unsafe fn replay_log(&self, state: &Mutex<huge::Dram>, base: NonNull<u64>) {
        let state = &mut *state.lock().unwrap();
        self.meta
            .log
            .replay(state, base, self.process_count, self.process_id, None);
    }

    pub(crate) unsafe fn size_log(
        &self,
        base: NonNull<u64>,
        pointer: NonNull<ffi::c_void>,
    ) -> usize {
        self.meta.log.size(base, pointer)
    }

    pub(crate) fn push(
        &self,
        id: thread::Id,
        meta: &mut region::owned::Meta,
        slabs: &slab::Slice<slab::Owned>,
        head: slab::Index,
        tail: slab::Index,
    ) {
        self.meta
            .free
            .push(id, meta, slabs, &self.meta.help, head, tail);
    }

    pub(crate) fn pop(
        &self,
        id: thread::Id,
        meta: &mut region::owned::Meta,
        slabs: &slab::Slice<slab::Owned>,
    ) -> Option<slab::Index> {
        if self.meta.free.is_empty(&self.meta.help) {
            return None;
        }

        self.meta.free.pop(id, meta, slabs, &self.meta.help)
    }
}

#[cfg(feature = "extend")]
impl<'raw> Shared<'raw> {
    pub(crate) fn barrier(&self) -> &Barrier {
        todo!()
    }

    pub(crate) fn request(&self) -> Option<Request> {
        todo!()
    }

    pub(crate) fn epoch(&self) -> Epoch {
        todo!()
    }
}

#[derive(Copy, Clone, Debug)]
pub(crate) enum Request {
    Map(u16),
    Extend(Epoch),
}

impl Index<root::Index> for Shared<'_> {
    type Output = Atomic<Option<slab::Offset>>;
    fn index(&self, index: root::Index) -> &Self::Output {
        &self.meta.roots[index]
    }
}

impl Index<thread::Id> for Shared<'_> {
    type Output = cas::Help;
    fn index(&self, id: thread::Id) -> &Self::Output {
        &self.meta.help[id]
    }
}

#[repr(C)]
pub(crate) struct Meta<'raw> {
    roots: root::Array,
    free: slab::GlobalStack<'raw>,
    help: thread::Array<cas::Help>,
    bump: cas::Detectable<Length>,
    pub(crate) log: huge::Cxl<2048>,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct Length(u32);

impl From<Length> for u32 {
    fn from(Length(length): Length) -> Self {
        length
    }
}

unsafe impl Packed for Length {
    const BITS: u8 = 32;

    fn pack(&self) -> u64 {
        self.0 as u64
    }

    fn unpack(value: u64) -> Self {
        Self(value as u32)
    }
}
