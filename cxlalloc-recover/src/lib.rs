pub mod clevel;
pub mod queue;
pub mod worker;

use core::cell::Cell;
use core::sync::atomic::AtomicBool;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::AtomicUsize;
use std::sync::Barrier;
use std::sync::LazyLock;
use std::sync::Mutex;
use std::sync::OnceLock;

pub const SEED: u64 = 0xdeadbeef;

pub static CRASH_VICTIM: AtomicUsize = AtomicUsize::new(0);
pub static CRASH_DETECT: Mutex<u64> = Mutex::new(0);
pub static CRASH_COUNT: AtomicU64 = AtomicU64::new(0);
pub static CRASH: AtomicU64 = AtomicU64::new(0);

pub static THREAD_COUNT: AtomicU64 = AtomicU64::new(0);
pub static OBJECT_COUNT: AtomicU64 = AtomicU64::new(0);

pub static CORES: LazyLock<Vec<core_affinity::CoreId>> =
    LazyLock::new(|| core_affinity::get_core_ids().unwrap());
pub static BLOCK: AtomicBool = AtomicBool::new(false);
pub static STOP: AtomicBool = AtomicBool::new(false);
pub static BARRIER: OnceLock<Barrier> = OnceLock::new();

pub static FINAL: AtomicU64 = AtomicU64::new(0);
pub static GLOBAL: AtomicU64 = AtomicU64::new(0);

pub static CACHE_COUNT: AtomicUsize = AtomicUsize::new(0);
pub static CACHE_SIZE: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    pub static LOCAL: Cell<u64> = const { Cell::new(0) };
}
