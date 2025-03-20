pub mod allocator;
pub mod barrier;
pub mod benchmark;
pub mod config;
pub mod index;

pub use allocator::Allocator;
pub use barrier::Barrier;
pub use index::Index;

use core::mem::MaybeUninit;
use std::fs::File;
use std::io;
use std::io::Read as _;
use std::io::Write as _;
use std::path::Path;

use serde::Deserialize;
use serde::Serialize;

pub struct Timer {}

#[derive(Deserialize, Serialize)]
pub struct Output {
    date: u64,
    process_id: usize,

    max_rss: u64,
    utime: u128,
    stime: u128,

    #[serde(flatten)]
    data: serde_json::Value,
}

pub(crate) struct Perf {
    ctl: File,
    ack: File,
}

impl Perf {
    pub fn new<C: AsRef<Path>, A: AsRef<Path>>(ctl: C, ack: A) -> Self {
        let ctl = ctl.as_ref();
        let ack = ack.as_ref();

        Self {
            ctl: File::options()
                .write(true)
                .open(ctl)
                .unwrap_or_else(|error| {
                    panic!(
                        "Failed to open perf control file {}: {}",
                        ctl.display(),
                        error,
                    )
                }),
            ack: File::options()
                .read(true)
                .open(ack)
                .unwrap_or_else(|error| {
                    panic!("Failed to open perf ack file {}: {}", ack.display(), error,)
                }),
        }
    }

    pub fn enable(&mut self) {
        self.ctl
            .write_all(b"enable\n\0")
            .expect("Failed to write to perf ctl file");
        self.wait();
    }

    pub fn disable(&mut self) {
        self.ctl
            .write_all(b"disable\n\0")
            .expect("Failed to write to perf ctl file");
        self.wait();
    }

    fn wait(&mut self) {
        match self.ack.read_exact(&mut [0u8; 5]) {
            Ok(()) => (),
            Err(error) => panic!("Failed to read from perf ack file: {}", error),
        }
    }
}

pub(crate) fn rusage() -> io::Result<libc::rusage> {
    unsafe {
        let mut rusage = MaybeUninit::<libc::rusage>::zeroed();
        match libc::getrusage(libc::RUSAGE_SELF, rusage.as_mut_ptr()) {
            0 => Ok(rusage.assume_init()),
            _ => Err(io::Error::last_os_error()),
        }
    }
}

pub(crate) fn timeval_as_nanos(time: libc::timeval) -> u128 {
    let s = time.tv_sec as u128 * 10u128.pow(9);
    let us = time.tv_usec as u128 * 10u128.pow(6);
    s + us
}
