use core::cell::UnsafeCell;
use core::ffi;
use core::num::NonZeroIsize;
use core::num::NonZeroU16;
use core::num::NonZeroUsize;
use core::ptr::NonNull;
use core::sync::atomic::AtomicBool;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::Ordering;
use std::collections::BTreeMap;

use crate::atomic::Packed;
use crate::thread;
use crate::Atomic;
use crate::SIZE_PAGE;

pub struct Log<const SIZE: usize> {
    lsn: Atomic<Lsn>,
    logs: thread::Array<[UnsafeCell<Entry>; SIZE]>,
}

impl<const SIZE: usize> Log<SIZE> {
    pub(crate) fn allocate(
        &self,
        id: thread::Id,
        process_count: usize,
        process_id: usize,
        hint: *mut ffi::c_void,
        size: usize,
    ) -> *mut ffi::c_void {
        let lsn = self.load_lsn();
        let index = self.next(id, process_count).unwrap();

        let size = size.next_multiple_of(SIZE_PAGE) + SIZE_PAGE;

        // TODO: map from file
        let address = match unsafe {
            libc::mmap64(
                hint,
                size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                -1,
                0,
            )
        } {
            libc::MAP_FAILED => panic!("Failed to mmap {:?} {:?}", hint, size),
            address => {
                // log::info!("Allocating {} at {:#x?}", size, address);
                address
            }
        };

        unsafe {
            crate::raw::region::mbind(address, size).expect("Failed to mbind");
        }

        let offset = NonZeroIsize::new(address as isize - hint as isize).unwrap();
        let size = NonZeroUsize::new(size).unwrap();

        unsafe {
            *self.logs[id][index].get() = Entry::Allocate {
                valid: AtomicBool::new(false),
                free_lsn: AtomicU64::new(0),
                free_id: Atomic::new(thread::Id::new(1)),
                lsn: lsn.lsn() as u64,
                offset,
                size,
            };
        }

        unsafe {
            address.cast::<Header>().write_volatile(Header {
                id,
                lsn: lsn.lsn() as u64,
                offset,
                size,
            });
        }

        match self.compare_exchange_lsn(lsn, Lsn::new(id, NonZeroU16::new(lsn.lsn() + 1).unwrap()))
        {
            Ok(()) => address.wrapping_byte_add(SIZE_PAGE),
            Err(_) => todo!(),
        }
    }

    pub(crate) fn free(
        &self,
        id: thread::Id,
        process_count: usize,
        process_id: usize,
        hint: *mut ffi::c_void,
        pointer: NonNull<ffi::c_void>,
    ) {
        let header = unsafe {
            pointer
                .as_ptr()
                .wrapping_byte_sub(SIZE_PAGE)
                .cast::<Header>()
                .read_volatile()
        };

        // log::info!("Header: {header:x?}");

        let lsn = self.load_lsn();
        let index = self.next(id, process_count).unwrap();

        unsafe {
            *self.logs[id][index].get() = Entry::Free {
                id,
                valid: AtomicBool::new(false),
                allocate: (header.id, header.lsn),
                lsn: lsn.lsn() as u64,
                ack: AtomicU64::new(0),
                offset: header.offset,
                size: header.size,
            };
        }

        match self.compare_exchange_lsn(lsn, Lsn::new(id, NonZeroU16::new(lsn.lsn() + 1).unwrap()))
        {
            Ok(()) => (),
            Err(_) => todo!(),
        }

        // crashing equivalent to unmap here?
        self.apply(hint, process_id, unsafe { &*self.logs[id][index].get() });
    }

    pub(crate) fn next(&self, id: thread::Id, process_count: usize) -> Option<usize> {
        self.logs[id]
            .iter()
            .position(|entry| match unsafe { &*entry.get() } {
                Entry::Empty => true,
                Entry::Allocate { valid, .. } | Entry::Free { valid, .. }
                    if !valid.load(Ordering::Acquire) =>
                {
                    // log::info!("Invalid: {entry:?}");
                    true
                }
                Entry::Allocate {
                    free_lsn, free_id, ..
                } => {
                    log::info!("Reuse allocate: {entry:?}");
                    if let Some(Entry::Free { ack, .. }) =
                        self.get(free_id.load(), free_lsn.load(Ordering::Acquire))
                    {
                        ack.load(Ordering::Acquire).count_ones() as usize == process_count
                    } else {
                        true
                    }
                }
                Entry::Free { ack, .. } => {
                    log::info!("Reuse free: {entry:?}");
                    ack.load(Ordering::Acquire).count_ones() as usize == process_count
                }
            })
    }

    fn replay(&self, from: u64, hint: *mut ffi::c_void, process_id: usize) {
        let entries = self
            .logs
            .into_iter()
            .flatten()
            .filter_map(|entry| match unsafe { &*entry.get() } {
                entry @ (Entry::Allocate { valid, lsn, .. } | Entry::Free { valid, lsn, .. })
                    if valid.load(Ordering::Acquire) && *lsn >= from =>
                {
                    Some((*lsn, entry))
                }
                _ => None,
            })
            .collect::<BTreeMap<_, _>>();

        for entry in entries.values() {
            self.apply(hint, process_id, entry);
        }
    }

    fn apply(&self, hint: *mut ffi::c_void, process_id: usize, entry: &Entry) {
        match entry {
            Entry::Empty => unreachable!(),
            Entry::Allocate { offset, size, .. } => unsafe {
                libc::mmap64(
                    hint.wrapping_byte_offset(offset.get()),
                    size.get(),
                    libc::PROT_WRITE | libc::PROT_READ,
                    libc::MAP_ANONYMOUS | libc::MAP_PRIVATE | libc::MAP_FIXED_NOREPLACE,
                    -1,
                    0,
                );
            },
            Entry::Free {
                ack, offset, size, ..
            } => {
                if ack.fetch_or(1 << process_id, Ordering::AcqRel) & (1 << process_id) > 0 {
                    return;
                }

                unsafe {
                    libc::munmap(hint.wrapping_byte_offset(offset.get()), size.get());
                }
            }
        }
    }

    fn get(&self, id: thread::Id, at: u64) -> Option<&Entry> {
        self.logs[id]
            .iter()
            .find_map(|entry| match unsafe { &*entry.get() } {
                Entry::Empty => None,
                entry @ (Entry::Allocate { valid, lsn, .. } | Entry::Free { valid, lsn, .. })
                    if *lsn == at =>
                {
                    Some(entry)
                }
                Entry::Allocate { .. } | Entry::Free { .. } => None,
            })
    }

    fn load_lsn(&self) -> Lsn {
        let current = self.lsn.load();

        if let Some(id) = current.id() {
            if let Some(entry) = self.get(id, current.lsn() as u64) {
                match entry {
                    Entry::Empty => unreachable!(),

                    Entry::Allocate { valid, .. } if !valid.load(Ordering::Acquire) => {
                        log::info!("Stamp {:?}", entry);
                        valid.fetch_or(true, Ordering::AcqRel);
                    }
                    Entry::Allocate { .. } => (),

                    Entry::Free {
                        valid,
                        allocate,
                        id,
                        lsn,
                        ..
                    } if !valid.load(Ordering::Acquire) => {
                        if let Some(Entry::Allocate {
                            free_id, free_lsn, ..
                        }) = self.get(allocate.0, allocate.1)
                        {
                            free_id.store(*id);
                            free_lsn.store(*lsn, Ordering::Release);
                        }

                        valid.fetch_or(true, Ordering::AcqRel);
                    }
                    Entry::Free { .. } => (),
                }
            }
        }

        current
    }

    fn compare_exchange_lsn(&self, old: Lsn, new: Lsn) -> Result<(), Lsn> {
        let now = self.load_lsn();

        if old != now {
            return Err(now);
        }

        self.lsn.compare_exchange(old, new).map(drop)
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
#[repr(transparent)]
struct Lsn(u64);

impl Lsn {
    fn new(id: thread::Id, lsn: NonZeroU16) -> Self {
        Self(id.pack() | ((lsn.get() as u64) << thread::Id::BITS))
    }

    fn id(&self) -> Option<thread::Id> {
        Option::<thread::Id>::unpack(self.0)
    }

    fn lsn(&self) -> u16 {
        (self.0 >> 16) as u16
    }
}

unsafe impl Packed for Lsn {
    const BITS: u8 = 64;

    fn pack(&self) -> u64 {
        self.0
    }

    fn unpack(value: u64) -> Self {
        Self(value)
    }
}

#[repr(C)]
#[derive(Debug)]
struct Header {
    id: thread::Id,
    lsn: u64,
    offset: NonZeroIsize,
    size: NonZeroUsize,
}

#[derive(Debug)]
enum Entry {
    Empty,
    Allocate {
        valid: AtomicBool,
        lsn: u64,

        // Forward pointer
        free_lsn: AtomicU64,
        free_id: Atomic<thread::Id>,

        // Mapping
        offset: NonZeroIsize,
        size: NonZeroUsize,
    },
    Free {
        valid: AtomicBool,
        id: thread::Id,
        lsn: u64,

        // Back pointer
        allocate: (thread::Id, u64),

        offset: NonZeroIsize,
        size: NonZeroUsize,
        ack: AtomicU64,
    },
}

// #[derive(Copy, Clone, Debug)]
// struct Entry {
//     free: bool,
//     seen: bool,
//     offset: Option<NonZeroI32>,
//     lsn: NonZeroU16,
// }
//
// const _: () = assert!(size_of::<Option<Entry>>() == 8);
//
// unsafe impl NonZero for Entry {}
//
// unsafe impl Packed for Entry {
//     const BITS: u8 = 64;
//
//     fn pack(&self) -> u64 {
//         (self.lsn.get() as u64)
//             | ((self.offset.map(NonZeroI32::get).unwrap_or(0) as u64) << 16)
//             | ((self.free as u64) << 48)
//             | ((self.seen as u64) << 49)
//     }
//
//     fn unpack(value: u64) -> Self {
//         Self {
//             seen: (value & (1 << 49)) > 0,
//             free: (value & (1 << 48)) > 0,
//             offset: NonZeroI32::new((value >> 16) as i32),
//             lsn: NonZeroU16::new(value as u16).unwrap(),
//         }
//     }
// }
