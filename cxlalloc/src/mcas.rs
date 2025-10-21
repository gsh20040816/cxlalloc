use core::ffi;
use core::ffi::CStr;
use core::marker::PhantomData;
use core::ptr;
use core::ptr::NonNull;
use core::sync::atomic::AtomicU16;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::Ordering;
use std::io;
use std::os::fd::AsRawFd as _;
use std::os::fd::FromRawFd as _;
use std::os::fd::OwnedFd;
use std::sync::OnceLock;

use crate::thread;

pub(crate) static MCAS: OnceLock<Mcas> = OnceLock::new();

thread_local! {
    pub(crate) static THREAD_ID: AtomicU16 = const { AtomicU16::new(0) };
}

// Temporary workaround for address resolution bug in hardware.
// Should support 64b alignment in theory?
#[repr(align(128))]
pub struct Atomic<T> {
    data: AtomicU64,
    _type: PhantomData<T>,
}

impl<T: ribbit::Pack> Atomic<T> {
    pub fn load(&self, ordering: Ordering) -> T {
        unsafe {
            ribbit::convert::loose_to_packed(ribbit::convert::loose_to_loose(
                self.data.load(ordering),
            ))
        }
    }

    pub fn store(&self, value: T, _ordering: Ordering) {
        let mut old = self.load(Ordering::Acquire);
        // Store through mCAS to avoid race condition with concurrent mCAS
        loop {
            match self.compare_exchange(old, value, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => break,
                Err(next) => old = next,
            }
        }
    }

    pub fn compare_exchange(
        &self,
        old: T,
        new: T,
        _success: Ordering,
        failure: Ordering,
    ) -> Result<T, T> {
        if mcas(
            self as *const _ as *mut _,
            ribbit::convert::loose_to_loose(ribbit::convert::packed_to_loose(old)),
            ribbit::convert::loose_to_loose(ribbit::convert::packed_to_loose(new)),
        ) {
            Ok(old)
        } else {
            Err(self.load(failure))
        }
    }
}

pub(crate) fn init_process() -> &'static Mcas {
    crate::mcas::MCAS.get_or_init(|| {
        let mut csr = Csr::new().unwrap();
        let mcas = Mcas::new(&mut csr).unwrap();

        // FIXME: assumes single process
        unsafe {
            libc::memset(mcas.target.virt.as_ptr().cast(), 0, Buffer::SIZE_TARGET);
        }

        mcas
    })
}

pub(crate) fn init_thread(id: thread::Id) {
    // HACK: need to provide mcas with global context
    THREAD_ID.with(|save| save.store(u16::from(id), Ordering::Relaxed));
}

#[inline(never)]
fn mcas(address: *mut u64, old: u64, new: u64) -> bool {
    let mcas = MCAS.get().unwrap();
    let phys = mcas.target.virt_to_phys(address);

    log::info!(
        "{:x?} mcas: v{:x?} p{:x?} o{:#x} n{:#x}",
        mcas,
        address,
        phys,
        old,
        new
    );

    let id = THREAD_ID.with(|id| id.load(Ordering::Relaxed) as u64);

    unsafe {
        let write = mcas.write.virt;
        let read = mcas.read.virt.cast::<u64>().byte_add(id as usize * 2 * 64);

        #[repr(C, align(64))]
        struct Input([u64; 4]);

        let buffer: Input = Input([old, new, phys, id * 2]);

        core::arch::asm! {
            "movdir64b {dst}, [{src}]",
            dst = in(reg) write.as_ptr(),
            src  = in(reg) &buffer,
        }

        // Make sure write makes it to NMP
        core::arch::x86_64::_mm_sfence();

        #[repr(C, align(16))]
        struct Output([u64; 2]);

        // Memory layout is [result, success]
        // But result can be garbage if not successful, so
        // it's not reliable. Must reload value from memory
        // when CAS fails to get an estimate of current value.
        let mut out = Output([0u64; 2]);

        core::arch::asm! {
            "movdqu xmm0, [{input}]",
            "movdqu [{output}], xmm0",
            input = in(reg) read.as_ptr(),
            output = in(reg) &mut out,
        }

        let success = out.0[1];
        log::warn!("{id} mcas result: {success}");
        success > 0
    }
}

const CXL_PCIE_BAR_PATH: &CStr = c"/sys/devices/pci0000:ab/0000:ab:00.1/resource2";

#[derive(Debug)]
pub struct Csr {
    address_virt: *mut u64,
}

impl Csr {
    const RD_BUFF: usize = 13;
    const WR_BUFF: usize = 14;

    pub fn new() -> io::Result<Self> {
        unsafe {
            let fd = match libc::open(CXL_PCIE_BAR_PATH.as_ptr(), libc::O_RDWR | libc::O_SYNC) {
                -1 => return Err(io::Error::last_os_error()),
                fd => OwnedFd::from_raw_fd(fd),
            };

            let address_virt = match libc::mmap(
                ptr::null_mut(),
                1 << 21,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd.as_raw_fd(),
                0,
            ) {
                libc::MAP_FAILED => return Err(io::Error::last_os_error()),
                address => address.cast(),
            };

            Ok(Self { address_virt })
        }
    }

    pub fn set(&mut self, index: usize, value: u64) {
        unsafe { self.address_virt.add(index).write_volatile(value) }
    }
}

#[derive(Debug)]
pub(crate) struct Mcas {
    read: Buffer,
    write: Buffer,
    target: Buffer,
}

unsafe impl Sync for Mcas {}
unsafe impl Send for Mcas {}

impl Mcas {
    pub(crate) fn new(csr: &mut Csr) -> io::Result<Self> {
        let target = Buffer::target(csr)?;
        let read = Buffer {
            phys: target.phys + Buffer::SIZE_TARGET as u64 - Buffer::SIZE_READ as u64,
            virt: unsafe {
                target
                    .virt
                    .byte_add(Buffer::SIZE_TARGET)
                    .byte_sub(Buffer::SIZE_READ)
            },
        };
        let write = Buffer {
            phys: read.phys - Buffer::SIZE_WRITE as u64,
            virt: unsafe { read.virt.byte_sub(Buffer::SIZE_WRITE) },
        };

        Ok(Self {
            // read: Buffer::read(csr)?,
            // write: Buffer::write(csr)?,
            read,
            write,
            target,
        })
    }

    pub(crate) fn address(&self) -> NonNull<shm::Page> {
        self.target.address()
    }
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct Buffer {
    phys: u64,
    virt: NonNull<shm::Page>,
}

impl Buffer {
    fn address(&self) -> NonNull<shm::Page> {
        self.virt
    }
}

unsafe impl Sync for Buffer {}
unsafe impl Send for Buffer {}

impl Buffer {
    const SIZE_READ: usize = 1 << 16;
    const SIZE_WRITE: usize = 1 << 16;
    pub(crate) const SIZE_TARGET: usize = 1 << 26;

    // pub fn read(csr: &mut Csr) -> io::Result<Self> {
    //     Self::map(
    //         csr,
    //         Some(Csr::RD_BUFF),
    //         c"/proc/mcas_rd_buff",
    //         Self::SIZE_READ,
    //     )
    // }
    //
    // pub fn write(csr: &mut Csr) -> io::Result<Self> {
    //     Self::map(
    //         csr,
    //         Some(Csr::WR_BUFF),
    //         c"/proc/mcas_wr_buff",
    //         Self::SIZE_WRITE,
    //     )
    // }

    pub fn target(csr: &mut Csr) -> io::Result<Self> {
        Self::map(csr, None, c"/proc/mcas_target_buff", Self::SIZE_TARGET)
    }

    fn virt_to_phys(&self, address: *mut u64) -> u64 {
        (address as u64)
            .checked_sub(self.virt.addr().get() as u64)
            .unwrap()
            + self.phys
    }

    fn map(csr: &mut Csr, index: Option<usize>, name: &CStr, size: usize) -> io::Result<Self> {
        unsafe {
            let fd = match libc::open(name.as_ptr(), libc::O_RDWR) {
                -1 => return Err(io::Error::last_os_error()),
                fd => OwnedFd::from_raw_fd(fd),
            };

            let mut address_phys = [0u8; 8];
            assert_eq!(
                libc::read(
                    fd.as_raw_fd(),
                    &mut address_phys as *mut u8 as *mut ffi::c_void,
                    8
                ),
                8
            );
            let address_phys = u64::from_ne_bytes(address_phys);

            if let Some(index) = index {
                csr.set(index, address_phys);
            }

            let address_virt = match libc::mmap(
                ptr::null_mut(),
                size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd.as_raw_fd(),
                0,
            ) {
                libc::MAP_FAILED => return Err(io::Error::last_os_error()),
                address => address,
            };

            Ok(Self {
                phys: address_phys,
                virt: NonNull::new(address_virt.cast::<shm::Page>()).unwrap(),
            })
        }
    }
}
