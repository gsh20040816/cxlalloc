use core::num::NonZeroUsize;
use core::mem::size_of;
use core::ptr;
use core::ptr::NonNull;
use core::sync::atomic::AtomicU32;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::Ordering;
use std::fs::File as StdFile;
use std::io;
use std::os::fd::AsRawFd as _;
use std::path::Path;

use shm::Numa;
use shm::Page;
use shm::Populate;

const MAGIC: u64 = 0x4358_4c46_4956_5031;
const VERSION: u32 = 1;
const METADATA_SIZE: usize = 1 << 20;
const SLOT_COUNT: usize = 4096;
const ID_SIZE: usize = 64;

#[repr(C, align(64))]
struct Header {
    magic: AtomicU64,
    version: AtomicU32,
    lock: AtomicU32,
    slot_count: AtomicU32,
    reserved: AtomicU32,
    next_offset: AtomicU64,
}

#[repr(C, align(64))]
struct Slot {
    state: AtomicU32,
    id_len: u32,
    offset: u64,
    size: u64,
    id: [u8; ID_SIZE],
}

#[derive(Debug)]
pub struct PlainIvshmem {
    device: StdFile,
    metadata: NonNull<u8>,
    base_offset: u64,
    capacity: u64,
}

unsafe impl Send for PlainIvshmem {}
unsafe impl Sync for PlainIvshmem {}

pub struct File {
    device: StdFile,
    size: NonZeroUsize,
    offset: i64,
    create: bool,
}

impl PlainIvshmem {
    pub fn open_resource(path: &Path, base_offset: u64, capacity: u64) -> io::Result<Self> {
        if capacity <= METADATA_SIZE as u64 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "ivshmem-plain capacity is smaller than metadata reservation",
            ));
        }

        let device = StdFile::options().read(true).write(true).open(path)?;
        let device_size = device.metadata()?.len();
        let end = base_offset
            .checked_add(capacity)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "ivshmem-plain range overflow"))?;
        if end > device_size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "ivshmem-plain range exceeds device size",
            ));
        }

        let metadata = unsafe {
            libc::mmap64(
                ptr::null_mut(),
                METADATA_SIZE,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                device.as_raw_fd(),
                base_offset as libc::off64_t,
            )
        };
        if metadata == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }

        let this = Self {
            device,
            metadata: NonNull::new(metadata.cast()).unwrap(),
            base_offset,
            capacity,
        };
        this.initialize();
        Ok(this)
    }

    pub(crate) fn name(&self) -> &'static str {
        "ivshmem-plain"
    }

    pub(crate) fn open(&self, id: &str, size: NonZeroUsize) -> crate::Result<File> {
        if id.len() > ID_SIZE {
            return Err(crate::Error::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                "ivshmem-plain region id is too long",
            )));
        }

        let size = NonZeroUsize::new(size.get().next_multiple_of(Page::SIZE)).unwrap();
        let _guard = self.lock();
        if let Some((offset, existing_size)) = self.find(id) {
            if existing_size < size.get() as u64 {
                return Err(crate::Error::Io(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "ivshmem-plain existing region is smaller than requested size",
                )));
            }
            return Ok(File {
                device: self.device.try_clone()?,
                size,
                offset: offset as i64,
                create: false,
            });
        }

        let offset = self.allocate(size.get() as u64)?;
        let slot = self.free_slot().ok_or_else(|| {
            crate::Error::Io(io::Error::new(
                io::ErrorKind::OutOfMemory,
                "ivshmem-plain metadata slot table is full",
            ))
        })?;
        unsafe {
            (*slot).id = [0; ID_SIZE];
            (&mut (*slot).id)[..id.len()].copy_from_slice(id.as_bytes());
            (*slot).id_len = id.len() as u32;
            (*slot).offset = offset;
            (*slot).size = size.get() as u64;
            (*slot).state.store(1, Ordering::Release);
        }

        Ok(File {
            device: self.device.try_clone()?,
            size,
            offset: offset as i64,
            create: true,
        })
    }

    pub(crate) fn unlink(&self, _id: &str) -> crate::Result<()> {
        Ok(())
    }

    fn initialize(&self) {
        if self.header().magic.load(Ordering::Acquire) == MAGIC {
            return;
        }

        let _guard = self.lock();
        let header = self.header();
        if header.magic.load(Ordering::Acquire) == MAGIC {
            return;
        }

        unsafe {
            ptr::write_bytes(self.slots().cast::<u8>(), 0, SLOT_COUNT * size_of::<Slot>());
        }
        header.next_offset.store(self.base_offset + METADATA_SIZE as u64, Ordering::Relaxed);
        header.slot_count
            .store(SLOT_COUNT.try_into().unwrap(), Ordering::Relaxed);
        header.version.store(VERSION, Ordering::Relaxed);
        header.magic.store(MAGIC, Ordering::Release);
    }

    fn header(&self) -> &Header {
        unsafe { self.metadata.cast::<Header>().as_ref() }
    }

    fn slots(&self) -> *mut Slot {
        unsafe { self.metadata.as_ptr().add(size_of::<Header>()).cast() }
    }

    fn slot(&self, index: usize) -> *mut Slot {
        unsafe { self.slots().add(index) }
    }

    fn lock(&self) -> LockGuard<'_> {
        let lock = &self.header().lock;
        while lock
            .compare_exchange_weak(0, 1, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
        LockGuard { lock }
    }

    fn find(&self, id: &str) -> Option<(u64, u64)> {
        for index in 0..SLOT_COUNT {
            let slot = self.slot(index);
            unsafe {
                if (*slot).state.load(Ordering::Acquire) != 1 {
                    continue;
                }
                if (*slot).id_len as usize != id.len() {
                    continue;
                }
                if &(&(*slot).id)[..id.len()] == id.as_bytes() {
                    return Some(((*slot).offset, (*slot).size));
                }
            }
        }
        None
    }

    fn free_slot(&self) -> Option<*mut Slot> {
        for index in 0..SLOT_COUNT {
            let slot = self.slot(index);
            unsafe {
                if (*slot).state.load(Ordering::Acquire) == 0 {
                    return Some(slot);
                }
            }
        }
        None
    }

    fn allocate(&self, size: u64) -> crate::Result<u64> {
        let header = self.header();
        let offset = align_up(header.next_offset.load(Ordering::Relaxed), Page::SIZE as u64);
        let next = offset.checked_add(size).ok_or_else(|| {
            crate::Error::Io(io::Error::new(
                io::ErrorKind::OutOfMemory,
                "ivshmem-plain allocation offset overflow",
            ))
        })?;
        if next > self.base_offset + self.capacity {
            return Err(crate::Error::Io(io::Error::new(
                io::ErrorKind::OutOfMemory,
                "ivshmem-plain backing range exhausted",
            )));
        }
        header.next_offset.store(next, Ordering::Release);
        Ok(offset)
    }
}

impl Drop for PlainIvshmem {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.metadata.as_ptr().cast(), METADATA_SIZE);
        }
    }
}

impl File {
    pub(crate) fn is_create(&self) -> bool {
        self.create
    }

    pub(crate) unsafe fn map(
        self,
        address: Option<NonNull<Page>>,
        numa: Option<Numa>,
        populate: Option<Populate>,
    ) -> crate::Result<NonNull<Page>> {
        let flags = libc::MAP_SHARED
            | address.map(|_| libc::MAP_FIXED).unwrap_or(0)
            | if matches!(populate, Some(Populate::PageTable)) {
                libc::MAP_POPULATE
            } else {
                0
            };
        let actual = unsafe {
            libc::mmap64(
                address
                    .map(NonNull::as_ptr)
                    .unwrap_or_else(ptr::null_mut)
                    .cast(),
                self.size.get(),
                libc::PROT_READ | libc::PROT_WRITE,
                flags,
                self.device.as_raw_fd(),
                self.offset as libc::off64_t,
            )
        };
        if actual == libc::MAP_FAILED {
            return Err(crate::Error::Mmap(io::Error::last_os_error()));
        }

        let actual = NonNull::new(actual.cast::<Page>()).unwrap();
        if let Some(expected) = address {
            assert_eq!(expected, actual);
        }
        if let Some(numa) = numa {
            numa.mbind(actual.as_ptr().cast(), self.size.get())?;
        }
        if matches!(populate, Some(Populate::Physical)) {
            unsafe {
                match libc::madvise(actual.as_ptr().cast(), self.size.get(), libc::MADV_POPULATE_WRITE) {
                    0 => (),
                    _ => return Err(crate::Error::Madvise(io::Error::last_os_error())),
                }
            }
        }

        Ok(actual)
    }
}

struct LockGuard<'a> {
    lock: &'a AtomicU32,
}

impl Drop for LockGuard<'_> {
    fn drop(&mut self) {
        self.lock.store(0, Ordering::Release);
    }
}

fn align_up(value: u64, alignment: u64) -> u64 {
    let mask = alignment - 1;
    (value + mask) & !mask
}
