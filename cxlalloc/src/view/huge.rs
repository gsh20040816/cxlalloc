use core::sync::atomic::AtomicBool;

use gcollections::ops::Bounded as _;
use gcollections::ops::Cardinality as _;
use gcollections::ops::Difference as _;
use gcollections::ops::Empty as _;
use gcollections::ops::Intersection as _;
use interval::interval_set::ToIntervalSet as _;
use interval::IntervalSet;

use crate::size;
use crate::slab;
use crate::thread;
use crate::view;
use crate::view::data;
use crate::Atomic;

pub(crate) struct Huge<'raw> {
    pub(crate) allocator: Allocator,
    pub(crate) shared: &'raw Shared,
    pub(crate) owned: &'raw thread::Array<Owned>,
    pub(crate) data: view::Data<'raw, size::Huge>,
}

impl<'raw> Huge<'raw> {
    pub(crate) fn new(
        shared: &'raw Shared,
        owned: &'raw thread::Array<Owned>,
        data: view::Data<'raw, size::Huge>,
    ) -> Self {
        Self {
            // FIXME: recover state
            allocator: Allocator::default(),
            shared,
            owned,
            data,
        }
    }
}

impl<'raw> Huge<'raw> {
    pub(crate) fn peek(&self, size: usize) -> Option<Descriptor> {
        self.allocator
            .free
            .iter()
            .find(|interval| interval.size() >= size)
            .map(|interval| interval.lower())
            .map(|offset| Descriptor {
                offset: self.data.checked_offset_to_offset(offset).unwrap(),
                id: self.allocator.id,
                size,
                next: None,
                free: AtomicBool::new(false),
            })
    }
}

pub(crate) struct Shared {
    slots: [Atomic<Option<thread::Id>>; 1024],
    next: Atomic<u64>,
}

impl Shared {
    pub(crate) fn claim(&self, id: thread::Id) -> slab::Index<size::Huge> {
        let next = self.next.load() as usize;

        for i in next..self.slots.len() {
            match self.slots[i].compare_exchange(None, Some(id)) {
                Ok(None) => {
                    log::info!("{} claimed slot {}", id, i);
                    self.next.store(i as u64 + 1);
                    todo!()
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
    pub(crate) head: Atomic<Option<data::Offset<size::Huge>>>,
}

impl core::ops::Index<thread::Id> for Huge<'_> {
    type Output = Descriptor;
    fn index(&self, id: thread::Id) -> &Self::Output {
        unsafe {
            self.data
                .offset_to_pointer::<Descriptor>(self.owned[id].head.load().unwrap())
                .as_ref()
        }
    }
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
    pub(crate) fn allocate(&mut self, offset: usize, size: usize) {
        let allocation = (offset, offset + size - 1).to_interval_set();
        assert_eq!(
            self.free.intersection(&allocation).size(),
            allocation.size(),
        );
        self.free = self.free.difference(&allocation);
    }

    pub(crate) fn free(&mut self, offset: usize, size: usize) {
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
    pub(crate) offset: view::data::Offset<size::Huge>,
    pub(crate) size: usize,
    pub(crate) next: Option<crate::Box<Descriptor>>,
    pub(crate) free: AtomicBool,
}
