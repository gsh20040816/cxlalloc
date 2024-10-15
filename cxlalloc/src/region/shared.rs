use core::alloc::Layout;
use core::ffi;
use core::ops::Index;
use core::ops::Range;
use core::ptr::NonNull;
use std::sync::Mutex;

use crate::atomic::Packed;
use crate::atomic::Version;
use crate::extend::Epoch;
use crate::huge;
use crate::raw;
use crate::region;
use crate::root;
use crate::slab;
use crate::thread;
use crate::Atomic;
use crate::Barrier;
use crate::SIZE_PAGE;

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

    pub(crate) fn allocate(&self, id: thread::Id, count: u32) -> Option<Range<slab::Index>> {
        let mut bump = self.meta.bump.load();
        let version = self.meta.stages[id].peek();
        self.meta.stages[id].prepare(version.next());

        let length = loop {
            if let Some(other) = bump.id() {
                self.meta.stages[other].notify(bump.version());
            }

            if bump.length().0 + count >= self.capacity {
                return None;
            }

            match self.meta.bump.compare_exchange(
                bump,
                Bump::new(id, version.next(), Length(bump.length().0 + count)),
            ) {
                Ok(_) => break bump.length(),
                Err(next) => bump = next,
            }
        };

        let start = slab::Index::from_length(Length(length.0));
        let end = slab::Index::from_length(Length(length.0 + count));
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
        slabs: &slab::Slice<slab::Owned>,
        head: slab::Index,
        tail: slab::Index,
    ) {
        self.meta
            .free
            .push(id, slabs, &self.meta.stages, head, tail);
    }

    pub(crate) fn pop(
        &self,
        id: thread::Id,
        meta: &mut region::owned::Meta,
        slabs: &slab::Slice<slab::Owned>,
    ) -> Option<slab::Index> {
        if self.meta.free.is_empty() {
            return None;
        }

        self.meta.free.pop(id, meta, slabs, &self.meta.stages)
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
    type Output = Help;
    fn index(&self, id: thread::Id) -> &Self::Output {
        &self.meta.stages[id]
    }
}

#[repr(C)]
pub(crate) struct Meta<'raw> {
    roots: root::Array,
    free: slab::GlobalStack<'raw>,
    stages: thread::Array<Help>,

    bump: Atomic<Bump>,
    pub(crate) log: huge::Cxl<1024>,
}

pub(crate) struct Help(Atomic<u64>);

impl Help {
    const FLAG: u64 = 1 << 63;

    pub(crate) fn peek(&self) -> Version {
        Version::unpack(self.0.load())
    }

    pub(crate) fn detect(&self) -> (Version, bool) {
        let value = self.0.load();
        (Version::unpack(value), value & Self::FLAG > 0)
    }

    pub(crate) fn prepare(&self, version: Version) {
        self.0.store(version.pack());
    }

    pub(crate) fn must_notify(&self, version: Version) -> bool {
        let (current, notified) = self.detect();
        current == version && !notified
    }

    pub(crate) fn notify(&self, version: Version) {
        let _ = self
            .0
            .compare_exchange(version.pack(), version.pack() | Self::FLAG);

        crate::flush(self, true);
        crate::fence();
    }
}

#[derive(Copy, Clone)]
struct Bump(u64);

impl Bump {
    fn new(id: thread::Id, version: Version, index: Length) -> Self {
        Self((id.pack() << 48) | (version.pack() << 32) | index.pack())
    }

    fn length(&self) -> Length {
        Packed::unpack(self.0)
    }

    fn version(&self) -> Version {
        Packed::unpack((self.0 >> 32) as u16 as u64)
    }

    fn id(&self) -> Option<thread::Id> {
        Packed::unpack(self.0 >> 48)
    }
}

unsafe impl Packed for Bump {
    const BITS: u8 = 64;

    fn pack(&self) -> u64 {
        self.0
    }

    fn unpack(value: u64) -> Self {
        Self(value)
    }
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
