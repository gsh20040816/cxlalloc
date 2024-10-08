use core::cell::UnsafeCell;
use core::cmp;
use core::ffi;
use core::fmt::Debug;
use core::num::NonZeroU32;
use core::num::NonZeroUsize;
use core::ptr::NonNull;
use core::sync::atomic::AtomicBool;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::Ordering;

use gcollections::ops::Bounded as _;
use gcollections::ops::Cardinality as _;
use gcollections::ops::Difference as _;
use gcollections::ops::Intersection as _;
use interval::interval_set::ToIntervalSet as _;
use interval::IntervalSet;

use crate::atomic::NonZero;
use crate::atomic::Packed;
use crate::thread;
use crate::Atomic;
use crate::SIZE_PAGE;

pub(crate) struct Dram {
    free: IntervalSet<usize>,

    next: u16,
}

impl Default for Dram {
    fn default() -> Self {
        Self {
            free: (0, (1 << 40) - 1).to_interval_set(),
            next: 0,
        }
    }
}

impl Dram {
    fn allocate(&self, size: usize) -> usize {
        self.free
            .iter()
            .find(|interval| interval.size() >= size)
            .unwrap()
            .lower()
    }

    fn mark_allocated(&mut self, offset: usize, size: NonZeroUsize, id: thread::Id, lsn: Lsn) {
        let allocation = (offset, offset + size.get() - 1).to_interval_set();
        assert_eq!(
            self.free.intersection(&allocation).size(),
            allocation.size(),
            "Local view inconsistent with global order on thread {id} lsn {lsn:?}",
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

pub(crate) struct Cxl<const SIZE: usize> {
    global: Atomic<Option<Tail>>,

    local: thread::Array<Atomic<Option<Lsn>>>,

    logs: thread::Array<[UnsafeCell<Entry>; SIZE]>,
}

impl<const SIZE: usize> Cxl<SIZE> {
    pub(crate) fn allocate(
        &self,
        state: &mut Dram,
        id: thread::Id,
        process_id: usize,
        base: NonNull<u64>,
        size: usize,
    ) -> NonNull<u64> {
        let index = self.next(state, id);
        let size = size.next_multiple_of(SIZE_PAGE) + SIZE_PAGE;
        let size = NonZeroUsize::new(size).unwrap();

        loop {
            let tail = self.tail(state, id, base, process_id);
            let next = Tail::new(
                id,
                index,
                tail.map(Tail::lsn).map(Lsn::next).unwrap_or(Lsn::MIN),
            );

            let offset = state.allocate(size.get());

            unsafe {
                *self.logs[id][index as usize].get() = Entry::Allocate {
                    valid: AtomicBool::new(false),
                    freed: AtomicBool::new(false),
                    lsn: next.lsn(),
                    offset,
                    size,
                };
            }

            match self.global.compare_exchange(tail, Some(next)) {
                Ok(_) => {
                    self.validate(next);
                    self.apply(state, base, process_id, id, next);
                    log::info!("Applied allocate {next:?}",);
                    return NonNull::new(
                        base.as_ptr()
                            .wrapping_byte_add(offset)
                            .wrapping_byte_add(SIZE_PAGE),
                    )
                    .unwrap();
                }
                Err(_) => log::info!("Conflict at {next:?}"),
            }
        }
    }

    pub(crate) fn free(
        &self,
        state: &mut Dram,
        id: thread::Id,
        process_id: usize,
        base: NonNull<u64>,
        pointer: NonNull<ffi::c_void>,
    ) {
        let tail = unsafe {
            pointer
                .as_ptr()
                .wrapping_byte_sub(SIZE_PAGE)
                .cast::<Atomic<Tail>>()
                .as_ref()
                .unwrap()
                .load()
        };

        let (offset, size) = match unsafe { &*self.logs[tail.id()][tail.index() as usize].get() } {
            Entry::Empty | Entry::Free { .. } => unreachable!(),
            Entry::Allocate {
                lsn,
                valid,
                freed: _,
                offset,
                size,
            } => {
                assert_eq!(*lsn, tail.lsn());
                assert!(valid.load(Ordering::Acquire));
                (*offset, *size)
            }
        };

        let index = self.next(state, id);

        loop {
            let tail = self
                .tail(state, id, base, process_id)
                .expect("Called free with no allocation log entry");

            let next = Tail::new(id, index, tail.lsn().next());

            unsafe {
                *self.logs[id][index as usize].get() = Entry::Free {
                    valid: AtomicBool::new(false),
                    acked: AtomicU64::new(0),
                    lsn: next.lsn(),
                    offset,
                    size,
                };
            }

            match self.global.compare_exchange(Some(tail), Some(next)) {
                Ok(_) => {
                    self.validate(next);
                    self.apply(state, base, process_id, id, next);
                    log::info!("Applied free {next:?}");
                    return;
                }
                Err(_) => log::info!("Conflict at {next:?}"),
            }
        }
    }

    pub(crate) fn next(&self, state: &mut Dram, id: thread::Id) -> u16 {
        const {
            assert!(SIZE < u16::MAX as usize);
        }

        let index = self.logs[id]
            .iter()
            .enumerate()
            .cycle()
            .skip(state.next as usize)
            .take(SIZE)
            .find_map(|(index, entry)| match unsafe { &*entry.get() } {
                Entry::Empty => Some(index),
                Entry::Allocate { valid, .. } | Entry::Free { valid, .. }
                    if !valid.load(Ordering::Acquire) =>
                {
                    log::info!("Reuse invalid at {entry:?}");
                    Some(index)
                }
                Entry::Allocate { freed, .. } if freed.load(Ordering::Acquire) => {
                    log::info!("Reuse allocate at {entry:?}");
                    Some(index)
                }
                Entry::Allocate { .. } => None,

                Entry::Free { lsn, .. }
                    if self.local.iter().all(|(_, tail)| tail.load() >= Some(*lsn)) =>
                {
                    log::info!("Reuse free at {entry:?}");
                    None
                }
                Entry::Free { .. } => None,
            })
            .map(|index| index as u16)
            .expect("Out of log slots");

        state.next = index + 1;
        index
    }

    fn tail(
        &self,
        state: &mut Dram,
        id: thread::Id,
        base: NonNull<u64>,
        process_id: usize,
    ) -> Option<Tail> {
        let tail = self.global.load()?;
        let seen = self.local[id].load();

        match Some(tail.lsn()).cmp(&seen) {
            cmp::Ordering::Less => unreachable!(),
            cmp::Ordering::Equal => return Some(tail),
            cmp::Ordering::Greater => self.validate(tail),
        }

        self.replay(state, id, base, process_id);
        Some(tail)
    }

    fn validate(&self, tail: Tail) {
        match unsafe { &*self.logs[tail.id()][tail.index() as usize].get() } {
            Entry::Empty => unreachable!(),

            Entry::Allocate { valid, lsn, .. } | Entry::Free { valid, lsn, .. }
                if *lsn == tail.lsn() && !valid.load(Ordering::Acquire) =>
            {
                valid.store(true, Ordering::Release);
            }

            Entry::Allocate { .. } | Entry::Free { .. } => (),
        }
    }

    fn replay(&self, state: &mut Dram, id: thread::Id, base: NonNull<u64>, process_id: usize) {
        let seen = self.local[id].load();
        let mut entries = self
            .logs
            .iter()
            .flat_map(|(id, entries)| {
                entries
                    .iter()
                    .enumerate()
                    .map(move |(index, entry)| (id, index, entry))
            })
            .filter_map(|(id, index, entry)| match unsafe { &*entry.get() } {
                Entry::Allocate { valid, lsn, .. } | Entry::Free { valid, lsn, .. }
                    if valid.load(Ordering::Acquire) && Some(*lsn) > seen =>
                {
                    Some(Tail::new(id, index as u16, *lsn))
                }
                _ => None,
            })
            .collect::<Vec<_>>();

        entries.sort_unstable();
        log::info!(
            "Replaying {:?}-{:?} for {}",
            seen,
            entries.last().map(|entry| entry.lsn()),
            id,
        );

        for entry in entries {
            self.apply(state, base, process_id, id, entry);
        }
    }

    fn apply(
        &self,
        state: &mut Dram,
        base: NonNull<u64>,
        process_id: usize,
        id: thread::Id,
        entry: Tail,
    ) {
        let lsn = match unsafe { &*self.logs[entry.id()][entry.index() as usize].get() } {
            Entry::Empty => unreachable!("Applied empty at {entry:?}"),
            Entry::Allocate { lsn, .. } if *lsn != entry.lsn() => entry.lsn(),
            Entry::Allocate {
                valid,
                lsn,
                offset,
                size,
                ..
            } => unsafe {
                assert!(valid.load(Ordering::Acquire));

                state.mark_allocated(*offset, *size, id, *lsn);

                let address = base
                    .as_ptr()
                    .cast::<ffi::c_void>()
                    .wrapping_byte_add(*offset);

                match libc::mmap64(
                    address,
                    size.get(),
                    libc::PROT_WRITE | libc::PROT_READ,
                    libc::MAP_ANONYMOUS | libc::MAP_PRIVATE | libc::MAP_FIXED_NOREPLACE,
                    -1,
                    0,
                ) {
                    libc::MAP_FAILED => {
                        log::info!(
                            "Mapping already established: {:#x?} ({:#x?})",
                            address,
                            size
                        );
                    }
                    actual => {
                        assert_eq!(address, actual);
                        crate::raw::region::mbind(actual, size.get()).unwrap();
                    }
                }

                address
                    .cast::<Atomic<Tail>>()
                    .as_ref()
                    .unwrap()
                    .store(entry);

                *lsn
            },
            Entry::Free {
                valid,
                lsn,
                acked,
                offset,
                size,
                ..
            } => {
                assert!(valid.load(Ordering::Acquire));
                assert_eq!(*lsn, entry.lsn());

                state.mark_deallocated(*offset, *size);

                // Note: assumes that the individual thread cannot crash between
                // this acknowledgement and the unmap call. A process crash is
                // fine, as the mapping will be destroyed.
                if acked.fetch_or(1 << process_id, Ordering::AcqRel) & (1 << process_id) > 0 {
                    return;
                }

                let address = base
                    .as_ptr()
                    .cast::<ffi::c_void>()
                    .wrapping_byte_add(*offset);

                let tail = unsafe { address.cast::<Atomic<Tail>>().as_ref().unwrap().load() };

                // Mark corresponding allocation log entry as freed
                match unsafe { &*self.logs[tail.id()][tail.index() as usize].get() } {
                    Entry::Allocate {
                        lsn,
                        valid,
                        freed,
                        offset: offset_,
                        size: size_,
                    } if *lsn == tail.lsn() => {
                        assert!(valid.load(Ordering::Acquire));
                        assert_eq!(offset, offset_);
                        assert_eq!(size, size_);
                        freed.store(true, Ordering::Release);
                    }
                    _ => (),
                }

                unsafe {
                    assert_eq!(libc::munmap(address, size.get()), 0);
                }

                *lsn
            }
        };

        self.local[id].store(Some(lsn));
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
struct Tail(u64);

impl Debug for Tail {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Tail")
            .field("id", &self.id())
            .field("index", &self.index())
            .field("lsn", &self.lsn())
            .finish()
    }
}

impl Tail {
    fn new(id: thread::Id, index: u16, lsn: Lsn) -> Self {
        Self(id.pack() | ((index as u64) << 16) | (lsn.pack() << 32))
    }

    fn id(self) -> thread::Id {
        thread::Id::unpack(self.0)
    }

    fn index(self) -> u16 {
        (self.0 >> 16) as u16
    }

    fn lsn(self) -> Lsn {
        Lsn::unpack(self.0 >> 32)
    }
}

unsafe impl NonZero for Tail {}

unsafe impl Packed for Tail {
    const BITS: u8 = 64;

    fn pack(&self) -> u64 {
        self.0
    }

    fn unpack(value: u64) -> Self {
        Self(value)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct Lsn(NonZeroU32);

impl Lsn {
    const MIN: Self = Self(NonZeroU32::MIN);

    fn next(self) -> Self {
        self.0.checked_add(1).map(Self).unwrap()
    }
}

unsafe impl NonZero for Lsn {}

unsafe impl Packed for Lsn {
    const BITS: u8 = 32;

    fn pack(&self) -> u64 {
        self.0.get() as u64
    }

    fn unpack(value: u64) -> Self {
        Self(NonZeroU32::new(value as u32).unwrap())
    }
}

#[derive(Debug)]
enum Entry {
    Empty,
    Allocate {
        lsn: Lsn,

        valid: AtomicBool,
        freed: AtomicBool,

        // Mapping
        offset: usize,
        size: NonZeroUsize,
    },
    Free {
        lsn: Lsn,

        valid: AtomicBool,
        acked: AtomicU64,

        offset: usize,
        size: NonZeroUsize,
    },
}
