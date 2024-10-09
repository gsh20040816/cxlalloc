use core::cell::UnsafeCell;
use core::cmp;
use core::ffi;
use core::fmt::Debug;
use core::num::NonZeroU32;
use core::num::NonZeroUsize;
use core::ptr;
use core::ptr::NonNull;
use core::sync::atomic::AtomicBool;
use core::sync::atomic::Ordering;
use std::sync::Mutex;

use gcollections::ops::Bounded as _;
use gcollections::ops::Cardinality as _;
use gcollections::ops::Difference as _;
use gcollections::ops::Intersection as _;
use interval::interval_set::ToIntervalSet as _;
use interval::IntervalSet;

use crate::atomic::NonZero;
use crate::atomic::Packed;
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

    logs: [[UnsafeCell<Entry>; SIZE]; COUNT_PROCESS],
}

pub fn spawn() {
    extern "C" {
        // https://man7.org/linux/man-pages/man3/pthread_attr_setstack.3.html
        fn pthread_attr_setstack(
            attr: *mut libc::pthread_attr_t,
            stackaddr: *mut ffi::c_void,
            stacksize: usize,
        );
    }

    unsafe {
        let mut attr = {
            let mut attr = core::mem::MaybeUninit::<libc::pthread_attr_t>::zeroed();
            libc::pthread_attr_init(attr.as_mut_ptr());
            attr.assume_init()
        };

        let address = match libc::mmap64(
            ptr::null_mut(),
            libc::PTHREAD_STACK_MIN,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_ANONYMOUS | libc::MAP_PRIVATE,
            -1,
            0,
        ) {
            libc::MAP_FAILED => panic!("Failed to create stack"),
            address => address,
        };

        let mut id = core::mem::MaybeUninit::<libc::pthread_t>::zeroed().assume_init();

        pthread_attr_setstack(&mut attr, address, libc::PTHREAD_STACK_MIN);
        libc::pthread_create(&mut id, &attr, run, ptr::null_mut());
    }
}

extern "C" fn run(_: *mut ffi::c_void) -> *mut ffi::c_void {
    log::info!("hello, world");
    ptr::null_mut()
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
        let size = NonZeroUsize::new(size).unwrap();

        loop {
            let tail = self.tail(state, base, process_count, process_id);
            let next = Tail::new(
                process_id,
                index,
                tail.map(Tail::lsn).map(Lsn::next).unwrap_or(Lsn::MIN),
            );

            let offset = state.allocate(size.get());

            unsafe {
                *self.logs[process_id][index as usize].get() = Entry::Allocate {
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
        let tail = unsafe {
            pointer
                .as_ptr()
                .wrapping_byte_sub(SIZE_PAGE)
                .cast::<Atomic<Tail>>()
                .as_ref()
                .unwrap()
                .load()
        };

        let (offset, size) =
            match unsafe { &*self.logs[tail.process_id()][tail.index() as usize].get() } {
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

        let state = &mut *state.lock().unwrap();
        let index = self.next(state, process_count, process_id);

        loop {
            let tail = self
                .tail(state, base, process_count, process_id)
                .expect("Called free with no allocation log entry");

            let next = Tail::new(process_id, index, tail.lsn().next());

            unsafe {
                *self.logs[process_id][index as usize].get() = Entry::Free {
                    valid: AtomicBool::new(false),
                    lsn: next.lsn(),
                    offset,
                    size,
                };
            }

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
            .find_map(|(index, entry)| match unsafe { &*entry.get() } {
                Entry::Empty => Some(index),
                Entry::Allocate { valid, .. } | Entry::Free { valid, .. }
                    if !valid.load(Ordering::Acquire) =>
                {
                    log::info!("Reuse invalid at {entry:?}");
                    Some(index)
                }
                entry @ Entry::Allocate { freed, lsn, .. }
                    if freed.load(Ordering::Acquire)
                        && self
                            .local
                            .iter()
                            .take(process_count)
                            .all(|tail| tail.load() >= Some(*lsn)) =>
                {
                    log::info!("Reuse allocate at {entry:?}");
                    Some(index)
                }
                Entry::Allocate { .. } => None,

                entry @ Entry::Free { lsn, .. }
                    if self
                        .local
                        .iter()
                        .take(process_count)
                        .all(|tail| tail.load() >= Some(*lsn)) =>
                {
                    log::info!("Reuse free at {entry:?}");
                    Some(index)
                }
                Entry::Free { .. } => None,
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
        match unsafe { &*self.logs[tail.process_id()][tail.index() as usize].get() } {
            Entry::Empty => unreachable!(),

            Entry::Allocate { valid, lsn, .. } | Entry::Free { valid, lsn, .. }
                if *lsn == tail.lsn() && !valid.load(Ordering::Acquire) =>
            {
                valid.store(true, Ordering::Release);
            }

            Entry::Allocate { .. } | Entry::Free { .. } => (),
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
            .filter_map(
                |(process_id, index, entry)| match unsafe { &*entry.get() } {
                    Entry::Allocate { valid, lsn, .. } | Entry::Free { valid, lsn, .. }
                        if valid.load(Ordering::Acquire) && Some(*lsn) > seen =>
                    {
                        Some(Tail::new(process_id, index as u16, *lsn))
                    }
                    _ => None,
                },
            )
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
        let lsn = match unsafe { &*self.logs[entry.process_id()][entry.index() as usize].get() } {
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

                state.mark_allocated(*offset, *size);

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
                        panic!(
                            "Mapping already established: {:#x?} ({:#x?})",
                            address, size
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
                offset,
                size,
                ..
            } => {
                assert!(valid.load(Ordering::Acquire));
                assert_eq!(*lsn, entry.lsn());

                state.mark_deallocated(*offset, *size);

                let address = base
                    .as_ptr()
                    .cast::<ffi::c_void>()
                    .wrapping_byte_add(*offset);

                let tail = unsafe { address.cast::<Atomic<Tail>>().as_ref().unwrap().load() };

                // Unmap for process
                unsafe {
                    log::info!("unmap {:#x?} ({:#x})", address, size.get());
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
                    match unsafe { &*self.logs[tail.process_id()][tail.index() as usize].get() } {
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
                }

                *lsn
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
        Self(process_id as u64 | ((index as u64) << 16) | (lsn.pack() << 32))
    }

    fn process_id(self) -> usize {
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

        offset: usize,
        size: NonZeroUsize,
    },
}
