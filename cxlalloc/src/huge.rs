use core::sync::atomic;

use core::ptr::NonNull;
use core::sync::atomic::AtomicBool;

use gcollections::ops::Bounded as _;
use gcollections::ops::Cardinality as _;
use gcollections::ops::Difference as _;
use gcollections::ops::Empty as _;
use gcollections::ops::Intersection as _;
use interval::interval_set::ToIntervalSet as _;
use interval::IntervalSet;

use crate::data;
use crate::raw::Backend;
use crate::size;
use crate::size::Bracket;
use crate::slab;
use crate::thread;
use crate::Atomic;
use crate::Data;

pub(crate) struct Huge<'raw> {
    pub(crate) allocator: Allocator,
    pub(crate) backend: &'raw Backend,
    pub(crate) shared: &'raw Shared,
    pub(crate) owned: &'raw thread::Array<Owned>,
    pub(crate) data: Data<'raw, size::Huge>,
}

impl<'raw> Huge<'raw> {
    pub(crate) fn new(
        backend: &'raw Backend,
        shared: &'raw Shared,
        owned: &'raw thread::Array<Owned>,
        data: Data<'raw, size::Huge>,
    ) -> Self {
        Self {
            // FIXME: recover state
            allocator: Allocator::default(),
            backend,
            shared,
            owned,
            data,
        }
    }

    pub(crate) fn trace(
        &self,
        id: thread::Id,
        data: &Data<'raw, size::Small>,
    ) -> impl Iterator<Item = &Descriptor> {
        let mut walk = self.get(id, data);
        std::iter::from_fn(move || {
            let next = walk?;
            walk = next.next.as_deref();
            Some(next)
        })
    }

    pub(crate) fn set(&self, id: thread::Id, data: &Data<'raw, size::Small>, head: &Descriptor) {
        let offset = data.pointer_to_offset(NonNull::from(head));
        self.owned[id].head.store(Some(offset))
    }

    pub(crate) fn claim(&mut self, id: thread::Id) {
        let slot = self.shared.claim(id);
        self.allocator.free(
            u32::from(slot) as usize * size::Huge::SIZE_SLAB,
            size::Huge::SIZE_SLAB,
        )
    }

    pub(crate) fn allocate(&mut self, size: usize) -> Option<Descriptor> {
        let descriptor = self
            .allocator
            .free
            .iter()
            .find(|interval| interval.size() >= size)
            .map(|interval| interval.lower())
            .inspect(|offset| {
                self.allocator.allocate(*offset, size);
            })
            .map(|offset| Descriptor {
                offset: self.data.checked_offset_to_offset(offset).unwrap(),
                id: self.allocator.id,
                size,
                next: None,
                free: AtomicBool::new(false),
            })?;

        Some(descriptor)
    }

    pub(crate) fn get(
        &self,
        id: thread::Id,
        data: &Data<'raw, size::Small>,
    ) -> Option<&Descriptor> {
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
    pub(crate) head: Atomic<Option<data::Offset<size::Small>>>,
}

pub(crate) struct Allocator {
    pub(crate) free: IntervalSet<usize>,
    pub(crate) id: usize,
}

impl Default for Allocator {
    fn default() -> Self {
        Self {
            free: IntervalSet::empty(),
            id: 0,
        }
    }
}

impl Allocator {
    fn allocate(&mut self, offset: usize, size: usize) {
        self.id += 1;
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
    pub(crate) id: usize,
    pub(crate) offset: data::Offset<size::Huge>,
    pub(crate) size: usize,
    pub(crate) next: Option<crate::Box<Descriptor>>,
    pub(crate) free: AtomicBool,
}

impl<'raw> Huge<'raw> {
    pub(crate) fn free(
        &self,
        data: &Data<'raw, size::Small>,
        offset_allocation: data::Offset<size::Huge>,
    ) {
        let slot = offset_allocation.into_index();
        let owner = self[slot].load().unwrap();
        let mut walk = self.get(owner, data).unwrap();

        while walk.offset != offset_allocation {
            walk = walk.next.as_ref().unwrap();
        }

        walk.free.store(true, atomic::Ordering::Relaxed);
    }
}
