use core::cmp;
use core::ffi;
use core::fmt::Debug;
use core::num::NonZeroU32;
use core::num::NonZeroUsize;
use core::ptr::NonNull;
use std::sync::Mutex;

use gcollections::ops::Bounded as _;
use gcollections::ops::Cardinality as _;
use gcollections::ops::Difference as _;
use gcollections::ops::Intersection as _;
use interval::interval_set::ToIntervalSet as _;
use interval::IntervalSet;

use crate::atomic::NonZero;
use crate::atomic::Packed;
use crate::stat;
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

    fn mark_allocated(&mut self, offset: usize, size: NonZeroUsize) {
        let allocation = (offset, offset + size.get() - 1).to_interval_set();
        assert_eq!(
            self.free.intersection(&allocation).size(),
            allocation.size(),
            "Local view inconsistent with global order",
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

const COUNT_PROCESS: usize = 64;

pub(crate) struct Cxl<const SIZE: usize> {
    global: Atomic<Option<Tail>>,

    local: [Atomic<Option<Lsn>>; COUNT_PROCESS],

    logs: [[Entry; SIZE]; COUNT_PROCESS],
}

impl<const SIZE: usize> Cxl<SIZE> {
    pub(crate) fn allocate(
        &self,
        state: &Mutex<Dram>,
        process_count: usize,
        process_id: usize,
        base: NonNull<u64>,
        size: usize,
    ) -> NonNull<u64> {
        let state = &mut *state.lock().unwrap();
        let index = self.next(state, process_count, process_id);
        let size = size.next_multiple_of(SIZE_PAGE) + SIZE_PAGE;
        stat::record_large(size);
        let size = NonZeroUsize::new(size).unwrap();

        loop {
            let tail = self.tail(state, base, process_count, process_id);
            let next = Tail::new(
                process_id,
                index,
                tail.map(Tail::lsn).map(Lsn::next).unwrap_or(Lsn::MIN),
            );

            let offset = state.allocate(size.get());

            self.logs[process_id][index as usize]
                .site
                .store(Site::new(offset, size.get()));
            self.logs[process_id][index as usize]
                .meta
                .store(Meta::allocate(next.lsn(), false, false));

            match self.global.compare_exchange(tail, Some(next)) {
                Ok(_) => {
                    self.validate(next);
                    self.apply(state, base, process_count, process_id, next);
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
        state: &Mutex<Dram>,
        process_count: usize,
        process_id: usize,
        base: NonNull<u64>,
        pointer: NonNull<ffi::c_void>,
    ) {
        let (offset, size) = unsafe { self.metadata(base, pointer) };

        let state = &mut *state.lock().unwrap();
        let index = self.next(state, process_count, process_id);

        loop {
            let tail = self
                .tail(state, base, process_count, process_id)
                .expect("Called free with no allocation log entry");

            let next = Tail::new(process_id, index, tail.lsn().next());

            self.logs[process_id][index as usize]
                .site
                .store(Site::new(offset, size.get()));
            self.logs[process_id][index as usize]
                .meta
                .store(Meta::free(next.lsn(), false));

            match self.global.compare_exchange(Some(tail), Some(next)) {
                Ok(_) => {
                    self.validate(next);
                    self.apply(state, base, process_count, process_id, next);
                    log::info!("Applied free {next:?}");
                    return;
                }
                Err(_) => log::info!("Conflict at {next:?}"),
            }
        }
    }

    pub(crate) unsafe fn size(&self, base: NonNull<u64>, pointer: NonNull<ffi::c_void>) -> usize {
        self.metadata(base, pointer).1.get() - SIZE_PAGE
    }

    unsafe fn metadata(
        &self,
        base: NonNull<u64>,
        pointer: NonNull<ffi::c_void>,
    ) -> (usize, NonZeroUsize) {
        let tail = pointer
            .as_ptr()
            .wrapping_byte_sub(SIZE_PAGE)
            .cast::<Atomic<Option<Tail>>>()
            .as_ref()
            .unwrap()
            .load()
            .unwrap();

        match self.logs[tail.process_id()][tail.index() as usize]
            .meta
            .load()
            .get()
        {
            None | Some(Kind::Free { .. }) => unreachable!(),
            Some(Kind::Allocate { lsn, valid, freed }) => {
                assert_eq!(lsn, tail.lsn());
                assert!(valid);
                assert!(!freed);

                let lo = self.logs[tail.process_id()][tail.index() as usize]
                    .site
                    .load();

                assert_eq!(
                    lo.offset(),
                    pointer.as_ptr() as usize - base.as_ptr() as usize - SIZE_PAGE,
                );
                (lo.offset(), NonZeroUsize::new(lo.size()).unwrap())
            }
        }
    }

    pub(crate) fn next(&self, state: &mut Dram, process_count: usize, process_id: usize) -> u16 {
        const {
            assert!(SIZE < u16::MAX as usize);
        }

        let index = self.logs[process_id]
            .iter()
            .enumerate()
            .cycle()
            .skip(state.next as usize)
            .take(SIZE)
            .find_map(|(index, entry)| match entry.meta.load().get() {
                None => Some(index),
                Some(Kind::Allocate { valid: false, .. } | Kind::Free { valid: false, .. }) => {
                    log::info!("Reuse invalid at {index}");
                    Some(index)
                }
                Some(Kind::Allocate {
                    lsn, freed: true, ..
                }) if self
                    .local
                    .iter()
                    .take(process_count)
                    .all(|tail| tail.load() >= Some(lsn)) =>
                {
                    log::info!("Reuse allocate at {index}");
                    Some(index)
                }
                Some(Kind::Allocate { .. }) => None,

                Some(Kind::Free { lsn, .. })
                    if self
                        .local
                        .iter()
                        .take(process_count)
                        .all(|tail| tail.load() >= Some(lsn)) =>
                {
                    log::info!("Reuse free at {index}");
                    Some(index)
                }
                Some(Kind::Free { .. }) => None,
            })
            .map(|index| index as u16)
            .expect("Out of log slots");

        state.next = (index + 1) % SIZE as u16;
        index
    }

    fn tail(
        &self,
        state: &mut Dram,
        base: NonNull<u64>,
        process_count: usize,
        process_id: usize,
    ) -> Option<Tail> {
        let tail = self.global.load()?;
        let seen = self.local[process_id].load();

        match Some(tail.lsn()).cmp(&seen) {
            cmp::Ordering::Less => unreachable!(),
            cmp::Ordering::Equal => return Some(tail),
            cmp::Ordering::Greater => self.validate(tail),
        }

        self.replay(state, base, process_count, process_id);
        Some(tail)
    }

    fn validate(&self, tail: Tail) {
        match self.logs[tail.process_id()][tail.index() as usize]
            .meta
            .load()
            .get()
        {
            None => unreachable!(),

            Some(Kind::Allocate { valid, lsn, freed }) if lsn == tail.lsn() && !valid => {
                let _ = self.logs[tail.process_id()][tail.index() as usize]
                    .meta
                    .compare_exchange(
                        Meta::allocate(lsn, false, freed),
                        Meta::allocate(lsn, true, freed),
                    );
            }
            Some(Kind::Allocate { .. }) => (),

            Some(Kind::Free { valid, lsn }) if lsn == tail.lsn() && !valid => {
                let _ = self.logs[tail.process_id()][tail.index() as usize]
                    .meta
                    .compare_exchange(Meta::free(lsn, false), Meta::free(lsn, true));
            }
            Some(Kind::Free { .. }) => (),
        }
    }

    fn replay(
        &self,
        state: &mut Dram,
        base: NonNull<u64>,
        process_count: usize,
        process_id: usize,
    ) {
        let seen = self.local[process_id].load();
        let mut entries = self
            .logs
            .iter()
            .enumerate()
            .flat_map(|(process_id, entries)| {
                entries
                    .iter()
                    .enumerate()
                    .map(move |(index, entry)| (process_id, index, entry))
            })
            .filter_map(|(process_id, index, entry)| match entry.meta.load().get() {
                Some(Kind::Allocate { valid, lsn, .. } | Kind::Free { valid, lsn })
                    if valid && Some(lsn) > seen =>
                {
                    Some(Tail::new(process_id, index as u16, lsn))
                }
                _ => None,
            })
            .collect::<Vec<_>>();

        entries.sort_unstable();
        log::info!(
            "Replaying {:?}-{:?} for {}",
            seen,
            entries.last().map(|entry| entry.lsn()),
            process_id,
        );

        for entry in entries {
            self.apply(state, base, process_count, process_id, entry);
        }
    }

    fn apply(
        &self,
        state: &mut Dram,
        base: NonNull<u64>,
        process_count: usize,
        process_id: usize,
        entry: Tail,
    ) {
        let lsn = match self.logs[entry.process_id()][entry.index() as usize]
            .meta
            .load()
            .get()
        {
            None => unreachable!("Applied empty at {entry:?}"),
            Some(Kind::Allocate { lsn, freed, .. }) if lsn != entry.lsn() || freed => entry.lsn(),
            Some(Kind::Allocate { valid, lsn, .. }) => unsafe {
                assert!(valid);

                let lo = self.logs[entry.process_id()][entry.index() as usize]
                    .site
                    .load();
                let offset = lo.offset();
                let size = NonZeroUsize::new(lo.size()).unwrap();

                state.mark_allocated(offset, size);

                let address = base
                    .as_ptr()
                    .cast::<ffi::c_void>()
                    .wrapping_byte_add(offset);

                match libc::mmap64(
                    address,
                    size.get(),
                    libc::PROT_WRITE | libc::PROT_READ,
                    libc::MAP_ANONYMOUS | libc::MAP_PRIVATE | libc::MAP_FIXED_NOREPLACE,
                    -1,
                    0,
                ) {
                    libc::MAP_FAILED => panic!(
                        "mmap {:#x?}-{:#x?} ({:#x?})",
                        address,
                        address.wrapping_byte_add(size.get()),
                        size
                    ),
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

                lsn
            },
            Some(Kind::Free { valid, lsn }) => {
                assert!(valid);
                assert_eq!(lsn, entry.lsn());

                let lo = self.logs[entry.process_id()][entry.index() as usize]
                    .site
                    .load();
                let offset = lo.offset();
                let size = NonZeroUsize::new(lo.size()).unwrap();

                state.mark_deallocated(offset, size);

                let address = base
                    .as_ptr()
                    .cast::<ffi::c_void>()
                    .wrapping_byte_add(offset);

                let tail = unsafe {
                    address
                        .cast::<Atomic<Option<Tail>>>()
                        .as_ref()
                        .unwrap()
                        .load()
                        .unwrap()
                };

                // Unmap for process
                unsafe {
                    log::info!(
                        "unmap {:#x?}-{:#x?} ({:#x})",
                        address,
                        address.wrapping_byte_add(size.get()),
                        size.get()
                    );
                    assert_eq!(libc::munmap(address, size.get()), 0);
                }

                // FIXME: last thread in process
                if self
                    .local
                    .iter()
                    .take(process_count)
                    .enumerate()
                    .filter(|(id, _)| *id != process_id)
                    .all(|(_, lsn)| lsn.load() >= Some(tail.lsn()))
                {
                    // Mark for reuse
                    match self.logs[tail.process_id()][tail.index() as usize]
                        .meta
                        .load()
                        .get()
                    {
                        Some(Kind::Allocate { lsn, valid, freed })
                            if lsn == tail.lsn() && !freed =>
                        {
                            assert!(valid);
                            let _ = self.logs[tail.process_id()][tail.index() as usize]
                                .meta
                                .compare_exchange(
                                    Meta::allocate(lsn, true, false),
                                    Meta::allocate(lsn, true, true),
                                );
                        }
                        _ => (),
                    }
                }

                lsn
            }
        };

        self.local[process_id].store(Some(lsn));
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
struct Tail(u64);

impl Debug for Tail {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Tail")
            .field("process_id", &self.process_id())
            .field("index", &self.index())
            .field("lsn", &self.lsn())
            .finish()
    }
}

impl Tail {
    fn new(process_id: usize, index: u16, lsn: Lsn) -> Self {
        assert!(process_id < 64);
        Self(process_id as u64 | ((index as u64) << 16) | (lsn.pack() << 32))
    }

    fn process_id(self) -> usize {
        assert!((self.0 as u16) < 64);
        self.0 as u16 as usize
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

struct Entry {
    meta: Atomic<Meta>,
    site: Atomic<Site>,
}

struct Meta(u64);

impl Meta {
    fn allocate(lsn: Lsn, valid: bool, freed: bool) -> Self {
        Self((lsn.pack() << 32) | ((valid as u64) << 31) | ((freed as u64) << 30) | 0b011)
    }

    fn free(lsn: Lsn, valid: bool) -> Self {
        Self((lsn.pack() << 32) | ((valid as u64) << 31) | 0b10)
    }

    fn get(&self) -> Option<Kind> {
        if self.0 == 0 {
            None
        } else if self.0 & 1 > 0 {
            Some(Kind::Allocate {
                lsn: Lsn::unpack(self.0 >> 32),
                valid: self.0 & (1 << 31) > 0,
                freed: self.0 & (1 << 30) > 0,
            })
        } else {
            assert_eq!(self.0 & 0b11, 0b10);
            Some(Kind::Free {
                lsn: Lsn::unpack(self.0 >> 32),
                valid: self.0 & (1 << 31) > 0,
            })
        }
    }
}

enum Kind {
    Allocate { lsn: Lsn, freed: bool, valid: bool },
    Free { lsn: Lsn, valid: bool },
}

struct Site(u64);

unsafe impl Packed for Meta {
    const BITS: u8 = 64;
    fn pack(&self) -> u64 {
        self.0
    }
    fn unpack(value: u64) -> Self {
        Self(value)
    }
}

unsafe impl Packed for Site {
    const BITS: u8 = 64;
    fn pack(&self) -> u64 {
        self.0
    }
    fn unpack(value: u64) -> Self {
        Self(value)
    }
}

impl Site {
    fn new(offset: usize, size: usize) -> Self {
        Self(
            (u32::try_from(offset / SIZE_PAGE).unwrap() as u64) << 32
                | (u32::try_from(size / SIZE_PAGE).unwrap() as u64),
        )
    }

    fn offset(&self) -> usize {
        (self.0 >> 32) as usize * SIZE_PAGE
    }

    fn size(&self) -> usize {
        (self.0 as u32) as usize * SIZE_PAGE
    }
}
