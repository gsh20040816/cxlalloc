use core::hash::Hash as _;
use core::hash::Hasher as _;
use core::ops::Range;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::Ordering;
use std::hash::DefaultHasher;

use clap::Parser;
use serde::Serialize;
use shm::Shm;

use crate::Allocator;
use crate::Backend;
use crate::Pointer as _;
use crate::benchmark;

#[derive(Clone, Parser, Serialize)]
pub struct Ycsb {}

#[derive(Debug)]
pub struct Insert {
    key: &'static str,
    record: &'static str,
}

// HACK: CXL-SHM doesn't support allocations larger than 1KiB (1_000B data + 24B header)
const MAX_SIZE: usize = 1_000;

pub struct Global {
    trace: Vec<Insert>,
    thread_total: usize,
    index: Shm<FlatMap>,
}

unsafe impl Sync for Global {}

impl<B: Backend> benchmark::Interface<B> for Ycsb {
    type Global = Global;
    type Local = Range<usize>;

    fn setup_process(
        &self,
        numa: usize,
        process_count: usize,
        _process_id: usize,
        thread_count: usize,
    ) -> Self::Global {
        let data = include_str!("../../ycsb-a.txt");

        let mut commands = Vec::new();
        for line in data.split('\n') {
            let Some(line) = line.strip_prefix("INSERT ") else {
                continue;
            };

            let (_table, line) = line.split_once(' ').unwrap();
            let (key, record) = line.split_once(' ').unwrap();
            commands.push(Insert { key, record })
        }

        Global {
            trace: commands,
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
        let len = global.trace.len() / global.thread_total;
        let start = thread_id * len;
        start..start + len
    }

    fn run_thread(
        &self,
        global: &Self::Global,
        local: &mut Self::Local,
        allocator: &mut B::Allocator,
    ) {
        let map = unsafe { global.index.address().as_ref() }.unwrap();
        let commands = &global.trace[local.clone()];

        for commands in commands {
            // HACK: shrink record for CXL-SHM
            let record_len = MAX_SIZE - commands.key.len();

            let pointer = allocator.allocate(commands.key.len() + record_len).unwrap();

            unsafe {
                // Copy key
                pointer
                    .as_ptr()
                    .cast::<u8>()
                    .copy_from_nonoverlapping(commands.key.as_bytes().as_ptr(), commands.key.len());

                // Copy record
                pointer
                    .as_ptr()
                    .cast::<u8>()
                    .byte_add(commands.key.len())
                    .copy_from_nonoverlapping(
                        commands.record[..record_len].as_bytes().as_ptr(),
                        record_len,
                    );
            }

            let mut hasher = DefaultHasher::new();
            commands.key.hash(&mut hasher);
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
