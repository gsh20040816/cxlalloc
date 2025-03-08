pub mod allocator;
pub mod barrier;
pub mod benchmark;
pub mod context;
pub mod index;

pub use allocator::Allocator;
pub use barrier::Barrier;
pub use benchmark::Benchmark;
pub use index::Index;

use core::cell::Cell;
use std::time::Instant;

use serde::Deserialize;
use serde::Serialize;

pub struct Timer {}

#[derive(Deserialize, Serialize)]
pub struct Metrics {
    process_id: usize,
    thread_id: usize,
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
