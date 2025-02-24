use core::hash::Hash as _;
use core::hash::Hasher as _;
use core::ops::Range;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::Ordering;
use std::path::PathBuf;

use clap::Parser;
use rapidhash::RapidHasher;
use serde::Serialize;
use shm::Shm;

use crate::Allocator;
use crate::Backend;
use crate::Pointer as _;
use crate::benchmark;

#[derive(Clone, Parser, Serialize)]
pub struct Ycsb {
    workload: PathBuf,
}

impl Ycsb {
    pub fn args(&self) -> Vec<String> {
        vec![
            "ycsb".to_owned(),
            self.workload.to_str().unwrap().to_owned(),
        ]
    }
}

// HACK: CXL-SHM doesn't support allocations larger than 1KiB (1_000B data + 24B header)
#[expect(unused)]
const MAX_SIZE: usize = 1_000;

pub struct Global {
    workload: ycsb::Workload,
    thread_total: usize,
    index: Shm<FlatMap>,
}

unsafe impl Sync for Global {}

impl<B: Backend> benchmark::Interface<B> for Ycsb {
    const NAME: &str = "ycsb";
    type Global = Global;
    type Local = Range<usize>;

    fn setup_process(
        &self,
        numa: usize,
        process_count: usize,
        _process_id: usize,
        thread_count: usize,
    ) -> Self::Global {
        let workload = std::fs::read_to_string(&self.workload).unwrap_or_else(|error| {
            panic!(
                "Failed to read workload file {:?}: {:?}",
                self.workload, error
            )
        });
        let workload = toml::from_str(&workload).unwrap();

        Global {
            workload,
            thread_total: process_count * thread_count,
            index: Shm::new(Some(numa), c"index".to_owned()).unwrap(),
        }
    }

    fn setup_thread(
        &self,
        global: &Self::Global,
        thread_id: usize,
        _allocator: &mut B::Allocator,
    ) -> Self::Local {
        let len = global.workload.operation_count() / global.thread_total;
        let start = thread_id * len;
        start..start + len
    }

    fn run_thread(
        &self,
        process_count: usize,
        _process_id: usize,
        thread_count: usize,
        thread_id: usize,
        global: &Self::Global,
        _local: &mut Self::Local,
        allocator: &mut B::Allocator,
    ) {
        let map = unsafe { global.index.address().as_ref() }.unwrap();
        let thread_total = process_count * thread_count;
        let field_count = global.workload.field_count();
        let mut loader = global.workload.loader(thread_total, thread_id);

        for _ in 0..global.workload.record_count() / thread_total {
            let key = loader.next_key();
            // FIXME: CXL-SHM max record size
            let pointer = allocator.allocate(8 + 96 * field_count).unwrap();

            let mut hasher = RapidHasher::default();
            key.hash(&mut hasher);
            let mut index = hasher.finish() as usize % map.0.len();

            loop {
                while map.0[index].load(Ordering::Acquire) > 0 {
                    index += 1;
                    index %= map.0.len();
                }

                if map.0[index]
                    .compare_exchange(0, pointer.as_u64(), Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    break;
                }
            }
        }
    }

    fn teardown_process(
        &self,
        _process_count: usize,
        process_id: usize,
        _thread_count: usize,
        mut global: Self::Global,
    ) {
        if process_id != 0 {
            return;
        }

        global.index.unlink().unwrap();
    }
}

struct FlatMap([AtomicU64; 1 << 20]);
