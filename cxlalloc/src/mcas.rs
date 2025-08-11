#![expect(unused)]

use core::ffi;
use core::ffi::CStr;
use core::marker::PhantomData;
use core::ptr;
use core::sync::atomic::AtomicU16;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::Ordering;
use std::io;
use std::os::fd::AsRawFd as _;
use std::os::fd::FromRawFd as _;
use std::os::fd::OwnedFd;
use std::sync::OnceLock;

pub(crate) static MCAS: OnceLock<Mcas> = OnceLock::new();
pub(crate) static TARGET: OnceLock<Buffer> = OnceLock::new();

thread_local! {
    pub(crate) static THREAD_ID: AtomicU16 = const { AtomicU16::new(0) };
}

#[repr(align(64))]
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

    pub fn compare_exchange(
        &self,
        old: T,
        new: T,
        _success: Ordering,
        _failure: Ordering,
    ) -> Result<T, T> {
        mcas(
            self as *const _ as *mut _,
            ribbit::convert::loose_to_loose(ribbit::convert::packed_to_loose(old)),
            ribbit::convert::loose_to_loose(ribbit::convert::packed_to_loose(new)),
        )
        .map(|old| unsafe {
            ribbit::convert::loose_to_packed(ribbit::convert::loose_to_loose(old))
        })
        .map_err(|old| unsafe {
            ribbit::convert::loose_to_packed(ribbit::convert::loose_to_loose(old))
        })
    }
}

fn mcas(address: *mut u64, old: u64, new: u64) -> Result<u64, u64> {
    let mcas = MCAS.get().unwrap();

    let target = TARGET.get().unwrap();
    let phys = target.virt_to_phys(address);
    let id = THREAD_ID.with(|id| id.load(Ordering::Relaxed) as u64);

    log::warn!(
        "{} {:?} {:?} mcas: v{:x?} p{:x?} o{} n{}",
        id,
        mcas,
        target,
        address,
        phys,
        old,
        new
    );

    let wr = mcas.write.address_virt.cast::<u64>();
    let rd = mcas.read.address_virt.cast::<u64>();

    unsafe {
        let mut buffer: Aligned = Aligned([old, new, phys, id * 2, 0, 0, 0, 0]);

        core::arch::asm! {
            "movdir64b [{dest}], {src}",
            dest = in(reg) wr,
            src  = in(reg) &mut buffer as *mut _,
        }

        // wr.write_volatile(old);
        // wr.add(1).write_volatile(new);
        // wr.add(2).write_volatile(phys);
        // wr.add(3).write_volatile(id * 2);

        // core::arch::x86_64::_mm_clflush(wr.cast());
        core::arch::x86_64::_mm_clflush(rd.cast());
        core::arch::x86_64::_mm_mfence();

        let rd = rd.byte_add(id as usize * 64);
        let mut out = [0u64; 2];

        core::arch::asm! {
            "movdqu xmm0, [{input}]",
            "movdqu [{output}], xmm0",
            input = in(reg) rd,
            output = in(reg) out.as_ptr(),
        }

        let result = out[0];
        let success = out[1];

        log::warn!("{id} mcas result: {result} {success}");

        match success {
            0 => Err(result),
            _ => Ok(result),
        }
    }
}

#[repr(C, align(64))]
struct Aligned([u64; 8]);

const CXL_PCIE_BAR_PATH: &CStr = c"/sys/devices/pci0000:27/0000:27:00.1/resource2";
const PAGE_SIZE: usize = 1 << 12;

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
pub struct Mcas {
    read: Buffer,
    write: Buffer,
}

unsafe impl Sync for Mcas {}
unsafe impl Send for Mcas {}

impl Mcas {
    pub fn new(csr: &mut Csr) -> io::Result<Self> {
        Ok(Self {
            read: Buffer::read(csr)?,
            write: Buffer::write(csr)?,
        })
    }
}

#[derive(Copy, Clone, Debug)]
pub struct Buffer {
    address_phys: *mut libc::c_void,
    address_virt: *mut libc::c_void,
}

unsafe impl Sync for Buffer {}
unsafe impl Send for Buffer {}

impl Buffer {
    pub fn read(csr: &mut Csr) -> io::Result<Self> {
        Self::map(
            csr,
            Some(Csr::RD_BUFF),
            c"/proc/mcas_rd_buff",
            PAGE_SIZE * 16,
        )
    }

    pub fn write(csr: &mut Csr) -> io::Result<Self> {
        Self::map(
            csr,
            Some(Csr::WR_BUFF),
            c"/proc/mcas_wr_buff",
            PAGE_SIZE * 16,
        )
    }

    pub fn target(csr: &mut Csr) -> io::Result<Self> {
        let buffer = Self::map(csr, None, c"/proc/mcas_target_buff", PAGE_SIZE * 16)?;

        unsafe {
            libc::memset(buffer.address_virt.cast(), 0, PAGE_SIZE * 16);
        }

        Ok(buffer)
    }

    fn virt_to_phys(&self, address: *mut u64) -> u64 {
        (address as u64)
            .checked_sub(self.address_virt as u64)
            .unwrap()
            + self.address_phys as u64
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
                address => address.cast(),
            };

            Ok(Self {
                address_phys: address_phys as *mut _,
                address_virt,
            })
        }
    }
}
