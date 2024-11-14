use core::cmp;
use core::ffi;
use core::num::NonZeroU32;
use core::num::NonZeroUsize;
use core::ptr::NonNull;
use core::sync::atomic::AtomicI64;
use core::sync::atomic::Ordering;
use std::sync::Mutex;

use gcollections::ops::Bounded as _;
use gcollections::ops::Cardinality as _;
use gcollections::ops::Difference as _;
use gcollections::ops::Intersection as _;
use interval::interval_set::ToIntervalSet as _;
use interval::IntervalSet;

use crate::raw::Backend;
use crate::raw::Region;
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
        backend: &Backend,
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
            let tail = self.tail(backend, state, base, process_count, process_id);
            let next = Tail::new(
                process_id as u16,
                index,
                tail.map(|tail| tail.lsn())
                    .map(Lsn::next)
                    .unwrap_or(Lsn::MIN),
            );

            let offset = state.allocate(size.get());

            self.logs[process_id][index as usize].site.store(Site::new(
                offset.try_into().unwrap(),
                size.get().try_into().unwrap(),
            ));
            self.logs[process_id][index as usize]
                .meta
                .store(Some(Meta::new(MetaUnpacked::Allocate(Allocate::new(
                    next.lsn(),
                    false,
                    false,
                )))));

            match self.global.compare_exchange(tail, Some(next)) {
                Ok(_) => {
                    self.validate(next);
                    self.apply(backend, state, base, process_count, process_id, next);
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
        backend: &Backend,
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
                .tail(backend, state, base, process_count, process_id)
                .expect("Called free with no allocation log entry");

            let next = Tail::new(process_id as u16, index, tail.lsn().next());

            self.logs[process_id][index as usize].site.store(Site::new(
                offset.try_into().unwrap(),
                size.get().try_into().unwrap(),
            ));
            self.logs[process_id][index as usize]
                .meta
                .store(Some(Meta::new(MetaUnpacked::Free(Free::new(
                    next.lsn(),
                    false,
                )))));

            match self.global.compare_exchange(Some(tail), Some(next)) {
                Ok(_) => {
                    self.validate(next);
                    self.apply(backend, state, base, process_count, process_id, next);
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

        match self.logs[tail.process_id() as usize][tail.index() as usize]
            .meta
            .load()
            .map(|meta| meta.unpack())
        {
            None | Some(MetaUnpacked::Free { .. }) => unreachable!(),
            Some(MetaUnpacked::Allocate(allocate)) => {
                let lsn = allocate.lsn();
                let valid = allocate.valid();
                let freed = allocate.freed();

                assert_eq!(lsn, tail.lsn());
                assert!(valid);
                assert!(!freed);

                let lo = self.logs[tail.process_id() as usize][tail.index() as usize]
                    .site
                    .load();

                assert_eq!(
                    lo.offset() as usize,
                    pointer.as_ptr() as usize - base.as_ptr() as usize - SIZE_PAGE,
                );
                (
                    lo.offset() as usize,
                    NonZeroU32::new(lo.size()).unwrap().try_into().unwrap(),
                )
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
            .find_map(
                |(index, entry)| match entry.meta.load().map(|meta| meta.unpack()) {
                    None => Some(index),
                    Some(MetaUnpacked::Allocate(allocate)) if !allocate.valid() => {
                        log::info!("Reuse invalid at {index}");
                        Some(index)
                    }
                    Some(MetaUnpacked::Free(free)) if !free.valid() => {
                        log::info!("Reuse invalid at {index}");
                        Some(index)
                    }
                    Some(MetaUnpacked::Allocate(allocate))
                        if allocate.freed()
                            && self
                                .local
                                .iter()
                                .take(process_count)
                                .all(|tail| tail.load() >= Some(allocate.lsn())) =>
                    {
                        log::info!("Reuse allocate at {index}");
                        Some(index)
                    }
                    Some(MetaUnpacked::Allocate { .. }) => None,

                    Some(MetaUnpacked::Free(free))
                        if self
                            .local
                            .iter()
                            .take(process_count)
                            .all(|tail| tail.load() >= Some(free.lsn())) =>
                    {
                        log::info!("Reuse free at {index}");
                        Some(index)
                    }
                    Some(MetaUnpacked::Free { .. }) => None,
                },
            )
            .map(|index| index as u16)
            .expect("Out of log slots");

        state.next = (index + 1) % SIZE as u16;
        index
    }

    fn tail(
        &self,
        backend: &Backend,
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

        self.replay(backend, state, base, process_count, process_id, seen);
        Some(tail)
    }

    fn validate(&self, tail: Tail) {
        match self.logs[tail.process_id() as usize][tail.index() as usize]
            .meta
            .load()
            .map(|meta| meta.unpack())
        {
            None => unreachable!(),

            Some(MetaUnpacked::Allocate(allocate))
                if allocate.lsn() == tail.lsn() && !allocate.valid() =>
            {
                let _ = self.logs[tail.process_id() as usize][tail.index() as usize]
                    .meta
                    .compare_exchange(
                        Some(Meta::new(MetaUnpacked::Allocate(Allocate::new(
                            allocate.lsn(),
                            false,
                            allocate.freed(),
                        )))),
                        Some(Meta::new(MetaUnpacked::Allocate(Allocate::new(
                            allocate.lsn(),
                            true,
                            allocate.freed(),
                        )))),
                    );
            }
            Some(MetaUnpacked::Allocate { .. }) => (),

            Some(MetaUnpacked::Free(free)) if free.lsn() == tail.lsn() && !free.valid() => {
                let _ = self.logs[tail.process_id() as usize][tail.index() as usize]
                    .meta
                    .compare_exchange(
                        Some(Meta::new(MetaUnpacked::Free(Free::new(free.lsn(), false)))),
                        Some(Meta::new(MetaUnpacked::Free(Free::new(free.lsn(), true)))),
                    );
            }
            Some(MetaUnpacked::Free { .. }) => (),
        }
    }

    pub(crate) fn replay(
        &self,
        backend: &Backend,
        state: &mut Dram,
        base: NonNull<u64>,
        process_count: usize,
        process_id: usize,
        seen: Option<Lsn>,
    ) {
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
            .filter_map(|(process_id, index, entry)| {
                match entry.meta.load().map(|meta| meta.unpack()) {
                    Some(MetaUnpacked::Allocate(allocate))
                        if allocate.valid() && Some(allocate.lsn()) > seen =>
                    {
                        Some(Tail::new(process_id as u16, index as u16, allocate.lsn()))
                    }

                    Some(MetaUnpacked::Free(free)) if free.valid() && Some(free.lsn()) > seen => {
                        Some(Tail::new(process_id as u16, index as u16, free.lsn()))
                    }
                    _ => None,
                }
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
            self.apply(backend, state, base, process_count, process_id, entry);
        }
    }

    fn apply(
        &self,
        backend: &Backend,
        state: &mut Dram,
        base: NonNull<u64>,
        process_count: usize,
        process_id: usize,
        entry: Tail,
    ) {
        let lsn = match self.logs[entry.process_id() as usize][entry.index() as usize]
            .meta
            .load()
            .map(|meta| meta.unpack())
        {
            None => unreachable!("Applied empty at {entry:?}"),
            Some(MetaUnpacked::Allocate(allocate))
                if allocate.lsn() != entry.lsn() || allocate.freed() =>
            {
                entry.lsn()
            }
            Some(MetaUnpacked::Allocate(allocate)) => unsafe {
                assert!(allocate.valid());

                let lo = self.logs[entry.process_id() as usize][entry.index() as usize]
                    .site
                    .load();
                let offset = lo.offset() as usize;
                let size = NonZeroU32::new(lo.size()).unwrap().try_into().unwrap();

                state.mark_allocated(offset, size);

                let address = base
                    .as_ptr()
                    .cast::<ffi::c_void>()
                    .wrapping_byte_add(offset);

                let region = backend
                    .allocate(
                        // FIXME: prefix with heap ID
                        format!("huge-{}", allocate.lsn()._0()),
                        NonNull::new(address),
                        size.get(),
                        0,
                    )
                    .unwrap_or_else(|_| panic!("Failed to allocate huge region"));

                address
                    .cast::<Atomic<Option<Tail>>>()
                    .as_ref()
                    .unwrap()
                    .store(Some(entry));

                address
                    .wrapping_byte_add(size_of::<Atomic<Option<Tail>>>())
                    .cast::<AtomicI64>()
                    .as_ref()
                    .unwrap()
                    .store(region.offset(), Ordering::Release);

                allocate.lsn()
            },
            Some(MetaUnpacked::Free(free)) => {
                assert!(free.valid());
                assert_eq!(free.lsn(), entry.lsn());

                let lo = self.logs[entry.process_id() as usize][entry.index() as usize]
                    .site
                    .load();
                let offset = lo.offset() as usize;
                let size = NonZeroUsize::new(lo.size() as usize).unwrap();

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

                let offset = unsafe {
                    address
                        .wrapping_byte_add(size_of::<Atomic<Option<Tail>>>())
                        .cast::<AtomicI64>()
                        .as_ref()
                        .unwrap()
                        .load(Ordering::Acquire)
                };

                // Unmap for process
                let region = Region::reconstruct(
                    format!("huge-{}", tail.lsn()._0()),
                    NonNull::new(address).unwrap().cast(),
                    size.get(),
                    offset,
                );

                backend.unmap(&region).expect("Failed to unmap");

                // Last process frees
                if self
                    .local
                    .iter()
                    .take(process_count)
                    .enumerate()
                    .filter(|(id, _)| *id != process_id)
                    .all(|(_, lsn)| lsn.load() >= Some(tail.lsn()))
                {
                    // Mark for reuse
                    match self.logs[tail.process_id() as usize][tail.index() as usize]
                        .meta
                        .load()
                        .map(|meta| meta.unpack())
                    {
                        meta @ Some(MetaUnpacked::Allocate(allocate))
                            if allocate.lsn() == tail.lsn() && !allocate.freed() =>
                        {
                            assert!(allocate.valid());

                            backend.free(&region).expect("Failed to free");

                            let _ = self.logs[tail.process_id() as usize][tail.index() as usize]
                                .meta
                                .compare_exchange(
                                    meta.map(Meta::new),
                                    Some(Meta::new(MetaUnpacked::Allocate(
                                        allocate.with_freed(true),
                                    ))),
                                );
                        }
                        _ => (),
                    }
                }

                free.lsn()
            }
        };

        self.local[process_id].store(Some(lsn));
    }
}

#[ribbit::pack(size = 64, nonzero, debug)]
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
struct Tail {
    process_id: u16,
    index: u16,
    #[ribbit(size = 32, nonzero)]
    lsn: Lsn,
}

#[ribbit::pack(size = 32, nonzero, debug)]
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Lsn(NonZeroU32);

impl Lsn {
    const MIN: Self = Self::new(NonZeroU32::MIN);

    fn next(self) -> Self {
        self._0().checked_add(1).map(Self::new).unwrap()
    }
}

struct Entry {
    meta: Atomic<Option<Meta>>,
    site: Atomic<Site>,
}

#[ribbit::pack(size = 64, nonzero)]
#[derive(Copy, Clone)]
enum Meta {
    #[ribbit(size = 34)]
    #[derive(Copy, Clone)]
    Allocate {
        #[ribbit(size = 32)]
        lsn: Lsn,
        valid: bool,
        freed: bool,
    },
    #[ribbit(size = 33)]
    #[derive(Copy, Clone)]
    Free {
        #[ribbit(size = 32)]
        lsn: Lsn,
        valid: bool,
    },
}

#[ribbit::pack(size = 64)]
#[derive(Copy, Clone)]
struct Site {
    offset: u32,
    size: u32,
}
