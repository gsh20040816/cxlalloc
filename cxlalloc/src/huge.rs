use core::num::NonZeroUsize;
use core::ops::Index;
use core::sync::atomic::AtomicBool;

use gcollections::ops::Bounded as _;
use gcollections::ops::Cardinality as _;
use gcollections::ops::Difference as _;
use gcollections::ops::Empty as _;
use gcollections::ops::Intersection as _;
use interval::interval_set::ToIntervalSet as _;
use interval::IntervalSet;

use crate::slab;
use crate::thread;
use crate::Atomic;

pub(crate) struct Allocator {
    free: IntervalSet<usize>,
    id: usize,
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
    pub(crate) fn allocate(&mut self, size: usize) -> Option<Descriptor> {
        self.free
            .iter()
            .find(|interval| interval.size() >= size)
            .map(|interval| interval.lower())
            .map(Offset)
            .map(|offset| {
                self.mark_allocated(offset.into(), size.try_into().unwrap());
                let id = self.id;
                self.id += 1;
                Descriptor {
                    offset,
                    id,
                    size,
                    next: None,
                    free: AtomicBool::new(false),
                }
            })
    }

    pub(crate) fn claim(&mut self, slot: Slot) {
        self.mark_deallocated(slot.0 * Slot::SIZE.get(), Slot::SIZE);
    }

    fn mark_allocated(&mut self, offset: usize, size: NonZeroUsize) {
        let allocation = (offset, offset + size.get() - 1).to_interval_set();
        assert_eq!(
            self.free.intersection(&allocation).size(),
            allocation.size(),
        );
        self.free = self.free.difference(&allocation);
    }

    fn mark_deallocated(&mut self, offset: usize, size: NonZeroUsize) {
        let allocation = (offset, offset + size.get() - 1).to_interval_set();
        if self.free.intersection(&allocation).size() > 0 {
            log::info!("Skipped freed allocation {offset:#x} ({size:#x})");
        }
        self.free.extend(allocation);
    }
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct Offset(usize);

impl From<Offset> for usize {
    fn from(offset: Offset) -> Self {
        offset.0
    }
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct Slot(usize);

impl Slot {
    const SIZE: NonZeroUsize = NonZeroUsize::new(1 << 34).unwrap();

    pub(crate) fn from_offset(offset: usize) -> Self {
        Self(offset / Self::SIZE.get())
    }
}

#[repr(C, align(64))]
#[derive(Debug)]
pub(crate) struct Descriptor {
    pub(crate) id: usize,
    pub(crate) offset: Offset,
    pub(crate) size: usize,
    pub(crate) next: Option<crate::Box<Descriptor>>,
    pub(crate) free: AtomicBool,
}

pub(crate) struct Array {
    owners: [Atomic<Option<thread::Id>>; 1024],
    pub(crate) descriptors: thread::Array<Atomic<Option<slab::Offset>>>,
    hint: Atomic<u64>,
}

impl Array {
    pub(crate) fn claim(&self, id: thread::Id) -> Slot {
        let hint = self.hint.load() as usize;

        for i in hint..self.owners.len() {
            match self.owners[i].compare_exchange(None, Some(id)) {
                Ok(None) => {
                    log::info!("{} claimed slot {}", id, i);
                    self.hint.store(i as u64 + 1);
                    return Slot(i);
                }
                Ok(Some(_)) => unreachable!(),
                Err(_) => (),
            }
        }

        panic!("Out of virtual address space")
    }
}

impl Index<Slot> for Array {
    type Output = Atomic<Option<thread::Id>>;
    fn index(&self, slot: Slot) -> &Self::Output {
        &self.owners[slot.0]
    }
}
