use core::ffi;
use core::mem;
use core::num::NonZeroUsize;

use core::ptr::NonNull;
use core::sync::atomic::AtomicUsize;
use core::sync::atomic::Ordering;
use std::collections::HashSet;

use gcollections::ops::Bounded as _;
use gcollections::ops::Cardinality as _;
use gcollections::ops::Difference as _;
use gcollections::ops::Empty as _;
use gcollections::ops::Intersection as _;
use interval::interval_set::ToIntervalSet as _;
use interval::IntervalSet;
use ribbit::private::u48;

use crate::allocator;
use crate::cache;
use crate::data;
use crate::raw::region;
use crate::raw::Backend;
use crate::size;
use crate::size::Bracket;
use crate::slab;
use crate::stat;
use crate::thread;
use crate::Atomic;
use crate::Data;

pub(crate) struct Huge<'raw> {
    allocator: Allocator,
    backend: &'raw Backend,
    region: &'raw region::Random,
    shared: &'raw Shared,
    owned: &'raw thread::Array<Owned>,
    data: Data<'raw, size::Huge>,
    stat: stat::thread::Recorder<size::Huge>,
}

impl<'raw> Huge<'raw> {
    pub(crate) fn new(
        backend: &'raw Backend,
        region: &'raw region::Random,
        shared: &'raw Shared,
        owned: &'raw thread::Array<Owned>,
        data: Data<'raw, size::Huge>,
    ) -> Self {
        Self {
            allocator: Allocator::default(),
            backend,
            region,
            shared,
            owned,
            data,
            stat: stat::thread::Recorder::default(),
        }
    }

    pub(crate) fn report(&self, id: thread::Id) -> impl Iterator<Item = stat::Report> + '_ {
        self.stat.report(id)
    }

    pub(crate) fn refresh(&self, data: &Data<size::Small>, id: thread::Id) {
        self.owned
            .iter()
            .filter_map(|owned| owned.head.load())
            .map(|offset| data.offset_to_pointer::<Descriptor>(offset))
            .map(|pointer| unsafe { pointer.as_ref() })
            .flat_map(|head| Self::trace(Some(head)))
            .filter(|descriptor| matches!(descriptor.state.load().unpack(), StateUnpacked::Free))
            .filter(|descriptor| self.owned[id].swap_remove(descriptor.offset))
            .for_each(|descriptor| self.unmap_descriptor(descriptor))
    }

    // Recover huge allocator DRAM state
    pub(crate) fn focus(&mut self, data: &Data<'raw, size::Small>, id: thread::Id) {
        self.shared
            .slots
            .iter()
            .enumerate()
            .filter_map(|(slot, owner)| match owner.load()? {
                claim if claim.id() == id => Some((slot, claim.slot_count())),
                _ => None,
            })
            .map(|(slot, slot_count)| (slab::Index::new_huge(slot), slot_count))
            .for_each(|(slot, slot_count)| self.allocator.claim(slot, slot_count.value() as usize));

        let walk = self.peek(data, id);
        let mut index = 0;

        Self::trace(walk)
            .inspect(|descriptor| index = index.max(descriptor.index))
            .filter(|descriptor| matches!(descriptor.state.load().unpack(), StateUnpacked::Live))
            .for_each(|descriptor| {
                self.allocator
                    .cover(u64::from(descriptor.offset) as usize, descriptor.size.get())
            });
        self.allocator.index = index;
    }

    pub(crate) fn allocate(
        &mut self,
        id: thread::Id,
        data: &Data<'raw, size::Small>,
        size: NonZeroUsize,
        out: &mut Descriptor,
        reuse: bool,
    ) -> *mut ffi::c_void {
        self.refresh(data, id);

        loop {
            match self.allocator.allocate(&self.data, size) {
                None => self.claim(id, size),
                Some(descriptor) => {
                    // save record somewhere
                    // will it conflict with link record?
                    //
                    // in order to link, we need to...
                    // - log link site
                    // - peek allocation
                    // - write to site
                    // - clear allocation
                    //
                    // what about allocation class?
                    // - if crash before writing to site, abort
                    // - if crash after writing to site, recover from site
                    //
                    // what about huge allocation?
                    // - need to log what
                    // - secondary link record
                    // - hard-code dedicated spot for huge
                    self.stat.record(
                        id,
                        stat::thread::Event::Allocate {
                            size: size.get() as u64,
                        },
                    );

                    out.state.store(State::new(StateUnpacked::Live));
                    out.index = descriptor.index;
                    out.offset = descriptor.offset;
                    out.size = descriptor.size;

                    if !reuse {
                        // point at previous head in data region
                        if let Some(prev) = self.peek(data, id) {
                            unsafe {
                                crate::Box::link(&mut out.next, prev);
                                cache::flush(out, cache::Invalidate::No);
                                cache::fence();
                            }
                        }

                        // update linked list of huge descriptors
                        self.set(id, data, out);
                    }

                    // FIXME: mark descriptor as allocated

                    // mmap huge allocation
                    self.owned[id].insert(out.offset);
                    self.map_descriptor(out).unwrap();

                    return self.data.offset_to_pointer(out.offset).as_ptr();
                }
            }
        }
    }

    pub(crate) fn free(
        &self,
        context: &mut allocator::Context,
        data: &Data<'raw, size::Small>,
        offset: data::Offset<size::Huge>,
    ) {
        self.refresh(data, context.id);

        let descriptor = self.find(data, offset).unwrap();
        self.stat.record(
            context.id,
            stat::thread::Event::Free {
                size: descriptor.size.get() as u64,
            },
        );

        self.unmap_descriptor(descriptor);
        self.owned[context.id].swap_remove(descriptor.offset);
        descriptor.state.store(State::new(StateUnpacked::Free));

        cache::flush(&descriptor.state, cache::Invalidate::Yes);

        cache::flush_cxl(descriptor);
        cache::fence_cxl();
    }

    pub(crate) fn class(
        &self,
        data: &Data<'raw, size::Small>,
        offset: data::Offset<size::Huge>,
    ) -> NonZeroUsize {
        self.find(data, offset).unwrap().size
    }

    pub(crate) fn checked_pointer_to_offset(
        &self,
        pointer: NonNull<ffi::c_void>,
    ) -> Option<data::Offset<size::Huge>> {
        match self.region.contains(pointer) {
            false => None,
            true => self.data.pointer_to_offset(pointer),
        }
    }

    pub(crate) fn try_map(
        &self,
        data: &Data<'raw, size::Small>,
        id: thread::Id,
        address: NonNull<ffi::c_void>,
    ) -> crate::Result<()> {
        let offset = self
            .checked_pointer_to_offset(address)
            .ok_or(crate::Error::OutOfBounds)?;

        let descriptor = self.find(data, offset).ok_or(crate::Error::OutOfBounds)?;

        self.owned[id].insert(descriptor.offset);
        self.map_descriptor(descriptor).map_err(crate::Error::from)
    }

    fn map_descriptor(&self, descriptor: &Descriptor) -> crate::Result<()> {
        log::info!("Map {:x?}", descriptor.offset);
        self.region.map(
            self.backend,
            u64::from(descriptor.offset) as usize,
            descriptor.size,
        )
    }

    fn unmap_descriptor(&self, descriptor: &Descriptor) {
        log::info!("Unmap {:x?}", descriptor.offset);
        self.region.unmap(
            self.backend,
            u64::from(descriptor.offset) as usize,
            descriptor.size,
        )
    }

    pub(crate) fn reuse(
        &mut self,
        data: &Data<'raw, size::Small>,
        id: thread::Id,
    ) -> Option<data::Offset<size::Small>> {
        let walk = self.peek(data, id);
        let mut safe = None;

        Self::trace(walk)
            .filter(|descriptor| match descriptor.state.load().unpack() {
                StateUnpacked::Live => false,
                // Protected by hazard pointer
                StateUnpacked::Free
                    if self
                        .owned
                        .iter()
                        .any(|owned| owned.contains(descriptor.offset)) =>
                {
                    false
                }
                StateUnpacked::Free => {
                    safe.get_or_insert(*descriptor);
                    true
                }
                StateUnpacked::Safe => {
                    safe.get_or_insert(*descriptor);
                    false
                }
            })
            .for_each(|descriptor| {
                self.allocator
                    .uncover(u64::from(descriptor.offset) as usize, descriptor.size.get());
                descriptor.state.store(State::new(StateUnpacked::Safe));
            });

        let safe = safe?;
        log::trace!("Reuse descriptor at {:#x?}", safe as *const _);
        data.pointer_to_offset(NonNull::from(safe))
    }

    fn find(
        &self,
        data: &Data<'raw, size::Small>,
        offset: data::Offset<size::Huge>,
    ) -> Option<&Descriptor> {
        let slot = offset.into_index();
        let claim = self[slot].load().unwrap();
        let walk = self.peek(data, claim.id());
        Self::trace(walk).find(|descriptor| descriptor.offset == offset)
    }

    fn trace(mut walk: Option<&'raw Descriptor>) -> impl Iterator<Item = &'raw Descriptor> {
        std::iter::from_fn(move || {
            let here = walk?;

            cache::flush_cxl(here);
            cache::fence_cxl();

            walk = here.next.as_deref();
            Some(here)
        })
    }

    fn set(&self, id: thread::Id, data: &Data<'raw, size::Small>, head: &Descriptor) {
        let offset = data.pointer_to_offset(NonNull::from(head));
        self.owned[id].head.store(offset)
    }

    fn claim(&mut self, id: thread::Id, size: NonZeroUsize) {
        let (slot, slot_count) = self.shared.claim(id, size);
        self.allocator.claim(slot, slot_count);
    }

    fn peek(&self, data: &Data<'raw, size::Small>, id: thread::Id) -> Option<&'raw Descriptor> {
        self.owned[id]
            .head
            .load()
            .map(|offset| data.offset_to_pointer::<Descriptor>(offset))
            .map(|pointer| unsafe { pointer.as_ref() })
    }
}

pub(crate) struct Shared {
    slots: [Atomic<Option<Claim>>; 1024],
    hint: Atomic<u64>,
}

#[ribbit::pack(size = 64, nonzero)]
pub struct Claim {
    #[ribbit(size = 16, nonzero)]
    id: thread::Id,
    slot_count: u48,
}

impl Shared {
    fn claim(&self, id: thread::Id, size: NonZeroUsize) -> (slab::Index<size::Huge>, usize) {
        let mut i = self.hint.load() as usize;

        let slot_count = size.get().next_multiple_of(size::Huge::SIZE_SLAB) / size::Huge::SIZE_SLAB;
        let claim = Claim::new(id, u48::new(slot_count as u64));

        while i + slot_count <= self.slots.len() {
            match self.slots[i].compare_exchange(None, Some(claim)) {
                Ok(Some(_)) | Err(None) => unreachable!(),
                Ok(None) => {
                    log::info!("Claimed slot {} ({})", i, slot_count);
                    self.hint.store((i + slot_count) as u64);
                    return (slab::Index::new_huge(i), slot_count);
                }
                Err(Some(claim)) => {
                    log::info!("Lost slot {} to {} ({})", i, claim.id(), claim.slot_count());
                    i += claim.slot_count().value() as usize;
                }
            }
        }

        panic!("Out of virtual address space")
    }
}

impl core::ops::Index<slab::Index<size::Huge>> for Huge<'_> {
    type Output = Atomic<Option<Claim>>;
    fn index(&self, index: slab::Index<size::Huge>) -> &Self::Output {
        &self.shared.slots[u32::from(index) as usize]
    }
}

pub(crate) struct Owned {
    head: Atomic<Option<data::Offset<size::Small>>>,

    len: AtomicUsize,
    hazards: [Atomic<Option<data::Offset<size::Huge>>>; 1024],
}

impl Owned {
    fn insert(&self, offset: data::Offset<size::Huge>) {
        if cfg!(feature = "validate") {
            assert!(!self.contains(offset));
        }

        let len = self.len.load(Ordering::Relaxed);
        self.hazards[len].store(Some(offset));
        self.len.store(len + 1, Ordering::Release);

        log::info!("Insert hazard pointer for {:#x?}", offset);
        self.invariant();
    }

    fn swap_remove(&self, offset: data::Offset<size::Huge>) -> bool {
        let len = self.len.load(Ordering::Relaxed);
        let Some(index) = self
            .hazards
            .iter()
            .take(len)
            .position(|hazard| hazard.load() == Some(offset))
        else {
            return false;
        };

        let last = self.hazards[len - 1].load();
        self.hazards[len - 1].store(None);
        self.hazards[index].store(last);
        self.len.store(len - 1, Ordering::Release);

        log::info!("Remove hazard pointer for {:#x?}", offset);
        self.invariant();
        true
    }

    fn contains(&self, offset: data::Offset<size::Huge>) -> bool {
        let len = self.len.load(Ordering::Relaxed);
        self.hazards
            .iter()
            .take(len)
            .any(|hazard| hazard.load() == Some(offset))
    }

    fn invariant(&self) {
        if !cfg!(feature = "validate") {
            return;
        }

        let len = self.len.load(Ordering::Relaxed);

        let unique = self
            .hazards
            .iter()
            .take(len)
            .map(|hazard| hazard.load().unwrap())
            .collect::<HashSet<_>>();

        assert_eq!(unique.len(), len);
    }
}

pub(crate) struct Allocator {
    free: IntervalSet<usize>,
    index: u64,
}

impl Default for Allocator {
    fn default() -> Self {
        Self {
            free: IntervalSet::empty(),
            index: 0,
        }
    }
}

impl Allocator {
    fn claim(&mut self, slot: slab::Index<size::Huge>, slot_count: usize) {
        self.uncover(
            u32::from(slot) as usize * size::Huge::SIZE_SLAB,
            slot_count * size::Huge::SIZE_SLAB,
        )
    }

    fn allocate(&mut self, data: &Data<size::Huge>, size: NonZeroUsize) -> Option<Descriptor> {
        self.free
            .iter()
            .find(|interval| interval.size() >= size.get())
            .map(|interval| interval.lower())
            .inspect(|offset| {
                self.cover(*offset, size.get());
                self.index += 1;
            })
            .map(|offset| Descriptor {
                offset: data.offset_to_offset(offset),
                size,
                index: self.index,
                next: None,
                state: Atomic::new(State::new(StateUnpacked::Live)),
            })
    }

    fn cover(&mut self, offset: usize, size: usize) {
        let allocation = (offset, offset + size - 1).to_interval_set();
        assert_eq!(
            self.free.intersection(&allocation).size(),
            allocation.size(),
        );
        self.free = self.free.difference(&allocation);
    }

    fn uncover(&mut self, offset: usize, size: usize) {
        let allocation = (offset, offset + size - 1).to_interval_set();
        if self.free.intersection(&allocation).size() > 0 {
            log::info!("Skipped freed allocation {offset:#x} ({size:#x})");
        }
        self.free.extend(allocation);
    }
}

#[repr(C, align(64))]
pub(crate) struct Descriptor {
    index: u64,
    offset: data::Offset<size::Huge>,
    size: NonZeroUsize,
    next: Option<crate::Box<Descriptor>>,
    state: Atomic<State>,
}

#[ribbit::pack(size = 2, debug)]
enum State {
    Live,
    Free,
    Safe,
}

impl Descriptor {
    pub(crate) const CLASS: size::Small = size::Small::new(mem::size_of::<Self>()).unwrap();
}
