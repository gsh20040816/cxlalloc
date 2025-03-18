pub mod allocator;
pub mod barrier;
pub mod benchmark;
pub mod config;
pub mod index;

pub use allocator::Allocator;
pub use barrier::Barrier;
pub use index::Index;

use core::cell::Cell;
use std::fs::File;
use std::io::Read as _;
use std::io::Write as _;
use std::path::Path;
use std::time::Instant;

use serde::Deserialize;
use serde::Serialize;

pub struct Timer {}

#[derive(Deserialize, Serialize)]
pub struct Metrics {
    process_id: usize,
    thread_id: usize,
    date: u64,
    time: u128,
}

thread_local! {
    static START: Cell<Option<Instant>> = const { Cell::new(None) };
}

impl Timer {
    fn new() -> Self {
        Self {}
    }

    fn start(&self) {
        START.set(Some(Instant::now()));
    }

    fn stop(&self) -> u128 {
        START
            .get()
            .map(|start| start.elapsed())
            .unwrap_or_default()
            .as_micros()
    }
}

pub struct Perf {
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
