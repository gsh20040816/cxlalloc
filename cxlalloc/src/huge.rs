use core::ffi;
use core::num::NonZeroUsize;

use core::ptr::NonNull;
use core::sync::atomic::AtomicBool;
use core::sync::atomic::Ordering;

use gcollections::ops::Bounded as _;
use gcollections::ops::Cardinality as _;
use gcollections::ops::Difference as _;
use gcollections::ops::Empty as _;
use gcollections::ops::Intersection as _;
use interval::interval_set::ToIntervalSet as _;
use interval::IntervalSet;

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

    // Recover huge allocator DRAM state
    pub(crate) fn focus(&mut self, data: &Data<'raw, size::Small>, id: thread::Id) {
        for slot in self
            .shared
            .slots
            .iter()
            .enumerate()
            .filter(|(_, owner)| owner.load() == Some(id))
            .map(|(slot, _)| slab::Index::new_huge(slot))
        {
            self.allocator.claim(slot);
        }

        let walk = self.peek(data, id);
        for descriptor in
            Self::trace(walk).filter(|descriptor| !descriptor.free.load(Ordering::Relaxed))
        {
            self.allocator
                .allocate(u64::from(descriptor.offset) as usize, descriptor.size.get());
        }
    }

    pub(crate) fn allocate(
        &mut self,
        id: thread::Id,
        data: &Data<'raw, size::Small>,
        size: NonZeroUsize,
        out: &mut Descriptor,
    ) -> *mut ffi::c_void {
        loop {
            match self.next(size) {
                None => self.claim(id),
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

                    *out = descriptor;

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

                    // FIXME: mark descriptor as allocated

                    // mmap huge allocation
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
        let descriptor = self.find(data, offset).unwrap();
        self.stat.record(
            context.id,
            stat::thread::Event::Free {
                size: descriptor.size.get() as u64,
            },
        );
        descriptor.free.store(true, Ordering::Relaxed);
        cache::flush(&descriptor.free, cache::Invalidate::Yes);
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
        address: NonNull<ffi::c_void>,
    ) -> crate::Result<()> {
        let offset = self
            .checked_pointer_to_offset(address)
            .ok_or(crate::Error::OutOfBounds)?;

        let descriptor = self.find(data, offset).ok_or(crate::Error::OutOfBounds)?;

        self.map_descriptor(descriptor).map_err(crate::Error::from)
    }

    fn map_descriptor(&self, descriptor: &Descriptor) -> crate::Result<()> {
        self.region.map(
            self.backend,
            u64::from(descriptor.offset) as usize,
            descriptor.size,
        )
    }

    fn next(&mut self, size: NonZeroUsize) -> Option<Descriptor> {
        let descriptor = self
            .allocator
            .free
            .iter()
            .find(|interval| interval.size() >= size.get())
            .map(|interval| interval.lower())
            .inspect(|offset| {
                self.allocator.allocate(*offset, size.get());
            })
            .map(|offset| Descriptor {
                offset: self.data.offset_to_offset(offset),
                size,
                index: self.allocator.index,
                next: None,
                free: AtomicBool::new(false),
            })?;

        Some(descriptor)
    }

    fn find(
        &self,
        data: &Data<'raw, size::Small>,
        offset: data::Offset<size::Huge>,
    ) -> Option<&Descriptor> {
        let slot = offset.into_index();
        let owner = self[slot].load().unwrap();
        let walk = self.peek(data, owner);
        Self::trace(walk).find(|descriptor| descriptor.offset == offset)
    }

    fn trace(mut walk: Option<&'raw Descriptor>) -> impl Iterator<Item = &'raw Descriptor> {
        std::iter::from_fn(move || {
            let here = walk?;
            walk = here.next.as_deref();
            Some(here)
        })
    }

    fn set(&self, id: thread::Id, data: &Data<'raw, size::Small>, head: &Descriptor) {
        let offset = data.pointer_to_offset(NonNull::from(head));
        self.owned[id].head.store(offset)
    }

    fn claim(&mut self, id: thread::Id) {
        let slot = self.shared.claim(id);
        self.allocator.claim(slot);
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
    slots: [Atomic<Option<thread::Id>>; 1024],
    next: Atomic<u64>,
}

impl Shared {
    fn claim(&self, id: thread::Id) -> slab::Index<size::Huge> {
        let next = self.next.load() as usize;

        for i in next..self.slots.len() {
            match self.slots[i].compare_exchange(None, Some(id)) {
                Ok(None) => {
                    log::info!("{} claimed slot {}", id, i);
                    self.next.store(i as u64 + 1);
                    return slab::Index::new_huge(i);
                }
                Ok(Some(_)) => unreachable!(),
                Err(_) => (),
            }
        }

        panic!("Out of virtual address space")
    }
}

impl core::ops::Index<slab::Index<size::Huge>> for Huge<'_> {
    type Output = Atomic<Option<thread::Id>>;
    fn index(&self, index: slab::Index<size::Huge>) -> &Self::Output {
        &self.shared.slots[u32::from(index) as usize]
    }
}

pub(crate) struct Owned {
    head: Atomic<Option<data::Offset<size::Small>>>,
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
    fn claim(&mut self, slot: slab::Index<size::Huge>) {
        self.free(
            u32::from(slot) as usize * size::Huge::SIZE_SLAB,
            size::Huge::SIZE_SLAB,
        )
    }

    fn allocate(&mut self, offset: usize, size: usize) {
        self.index += 1;
        let allocation = (offset, offset + size - 1).to_interval_set();
        assert_eq!(
            self.free.intersection(&allocation).size(),
            allocation.size(),
        );
        self.free = self.free.difference(&allocation);
    }

    fn free(&mut self, offset: usize, size: usize) {
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
    free: AtomicBool,
}
