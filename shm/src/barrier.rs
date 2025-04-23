use core::mem::MaybeUninit;
use std::ffi::CString;

use bon::bon;

use crate::Shm;
use crate::try_pthread;

pub struct Barrier(Shm<libc::pthread_barrier_t>);

unsafe impl Sync for Barrier {}
unsafe impl Send for Barrier {}

#[bon]
impl Barrier {
    #[builder]
    pub fn new(
        name: CString,
        #[builder(default)] create: bool,
        thread_count: u32,
    ) -> crate::Result<Self> {
        let inner = Shm::<libc::pthread_barrier_t>::builder()
            .name(name)
            .create(create)
            .build()?;

        if create {
            let mut attr = unsafe {
                let mut attr = MaybeUninit::<libc::pthread_barrierattr_t>::zeroed();
                try_pthread!(libc::pthread_barrierattr_init(attr.as_mut_ptr()))?;
                try_pthread!(libc::pthread_barrierattr_setpshared(
                    attr.as_mut_ptr(),
                    libc::PTHREAD_PROCESS_SHARED
                ))?;
                attr.assume_init()
            };

            unsafe {
                try_pthread!(libc::pthread_barrier_init(
                    inner.address().as_ptr(),
                    &attr,
                    thread_count
                ))?;
            }

            unsafe {
                assert_eq!(libc::pthread_barrierattr_destroy(&mut attr), 0);
            }
        }

        Ok(Self(inner))
    }

    pub fn wait(&self) -> crate::Result<bool> {
        match unsafe { libc::pthread_barrier_wait(self.0.address().as_ptr()) } {
            libc::PTHREAD_BARRIER_SERIAL_THREAD => Ok(true),
            0 => Ok(false),
            error => Err(crate::Error::Libc {
                name: "pthread_barrier_wait",
                source: std::io::Error::from_raw_os_error(error),
            }),
        }
    }

    pub fn unlink(&mut self) -> crate::Result<()> {
        unsafe { try_pthread!(libc::pthread_barrier_destroy(self.0.address().as_ptr()))? }
        self.0.unlink()
    }
}
