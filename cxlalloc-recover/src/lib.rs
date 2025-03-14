pub mod clevel;
pub mod queue;

use core::cell::Cell;
use core::sync::atomic::AtomicBool;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::AtomicUsize;
use std::sync::Barrier;
use std::sync::Mutex;
use std::sync::OnceLock;

pub const SEED: u64 = 0xdeadbeef;
pub static CRASH_THREAD: AtomicUsize = AtomicUsize::new(0);
pub static CRASH: Mutex<Vec<u64>> = Mutex::new(Vec::new());

pub static COUNT_THREADS: AtomicU64 = AtomicU64::new(0);
pub static COUNT_OBJECTS: AtomicU64 = AtomicU64::new(0);
pub static BLOCK: AtomicBool = AtomicBool::new(false);

pub static STOP: AtomicBool = AtomicBool::new(false);
pub static BARRIER: OnceLock<Barrier> = OnceLock::new();

pub static FINAL: AtomicU64 = AtomicU64::new(0);
pub static GLOBAL: AtomicU64 = AtomicU64::new(0);

thread_local! {
    pub static LOCAL: Cell<u64> = const { Cell::new(0) };
}
