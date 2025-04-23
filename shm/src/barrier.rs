use core::mem::MaybeUninit;
use std::ffi::CString;

use bon::bon;

use crate::Shm;

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
                assert_eq!(libc::pthread_barrierattr_init(attr.as_mut_ptr()), 0);
                assert_eq!(
                    libc::pthread_barrierattr_setpshared(
                        attr.as_mut_ptr(),
                        libc::PTHREAD_PROCESS_SHARED
                    ),
                    0
                );
                attr.assume_init()
            };

            unsafe {
                assert_eq!(
                    libc::pthread_barrier_init(inner.address_mut(), &attr, thread_count),
                    0
                );
            }

            unsafe {
                assert_eq!(libc::pthread_barrierattr_destroy(&mut attr), 0);
            }
        }

        Ok(Self(inner))
    }

    pub fn wait(&self) -> bool {
        match unsafe { libc::pthread_barrier_wait(self.0.address_mut()) } {
            libc::PTHREAD_BARRIER_SERIAL_THREAD => true,
            0 => false,
            error => panic!("Failed to wait on barrier: {}", error),
        }
    }

    pub fn unlink(&mut self) -> crate::Result<()> {
        unsafe { assert_eq!(libc::pthread_barrier_destroy(self.0.address_mut()), 0) }
        self.0.unlink()
    }
}
