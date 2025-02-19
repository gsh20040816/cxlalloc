use core::ffi;
use core::hash::Hash as _;
use core::hash::Hasher as _;
use core::hint;
use core::ops::Range;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::Ordering;
use std::hash::DefaultHasher;

use clap::Parser;
use serde::Serialize;

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

impl<B: Backend> benchmark::Interface<B> for Ycsb {
    type Global = (Vec<Insert>, usize);
    type Local = (*mut ffi::c_void, Range<usize>);

    fn setup_process(&self, process_count: usize, _: usize, thread_count: usize) -> Self::Global {
        let data = include_str!("../../ycsb-a.txt");

        let mut commands = Vec::new();
        for line in data.split('\n') {
            let Some(line) = line.strip_prefix("INSERT ") else {
                continue;
            };

            let (_table, line) = line.split_once(' ').unwrap();
            let (key, record) = line.split_once(' ').unwrap();
            // data = data.strip_prefix('[').unwrap();
            //
            // let mut record = Vec::new();
            // for _ in 0..10 {
            //     data = data.strip_prefix(' ').unwrap();
            //     let name = &data[..6];
            //     let value = &data[7..107];
            //     data = &data[107..];
            //     record.push(Field { name, value });
            // }

            commands.push(Insert { key, record })
        }

        (commands, process_count * thread_count)
    }

    fn setup_thread(
        &self,
        (commands, thread_count): &Self::Global,
        thread_id: usize,
        allocator: &mut B::Allocator,
    ) -> Self::Local {
        let map: <B::Allocator as Allocator>::Ptr = if thread_id == 0 {
            let pointer = if std::any::type_name::<B::Allocator>().contains("cxl_shm") {
                // HACK: relies on CXL-SHM allocating contiguous memory for consecutive requests
                // for the same size class
                let count = std::mem::size_of::<FlatMap>().next_multiple_of(MAX_SIZE) / MAX_SIZE;
                let head = allocator.allocate(MAX_SIZE).unwrap();
                for _ in 1..count {
                    allocator.allocate(MAX_SIZE).unwrap();
                }
                head
            } else {
                allocator.allocate(std::mem::size_of::<FlatMap>()).unwrap()
            };

            // allocator.set_root(pointer);
            // allocator.get_root().unwrap()
            todo!()
        } else {
            // loop {
            //     match allocator.get_root() {
            //         Some(root) => break root,
            //         None => hint::spin_loop(),
            //     }
            // }
            todo!()
        };

        let len = commands.len() / thread_count;
        let start = thread_id * len;
        (map.as_ptr(), start..start + len)
    }

    fn run_thread(
        &self,
        (commands, _): &Self::Global,
        (map, range): &mut Self::Local,
        allocator: &mut B::Allocator,
    ) {
        let map = unsafe { map.cast::<FlatMap>().as_ref().unwrap() };

        for insert in &commands[range.clone()] {
            // HACK: shrink record for CXL-SHM
            let record_len = 1_000 - insert.key.len();

            let pointer = allocator.allocate(insert.key.len() + record_len).unwrap();

            unsafe {
                // Copy key
                pointer
                    .as_ptr()
                    .cast::<u8>()
                    .copy_from_nonoverlapping(insert.key.as_bytes().as_ptr(), insert.key.len());

                // Copy record
                pointer
                    .as_ptr()
                    .cast::<u8>()
                    .byte_add(insert.key.len())
                    .copy_from_nonoverlapping(
                        insert.record[..record_len].as_bytes().as_ptr(),
                        record_len,
                    );
            }

            let mut hasher = DefaultHasher::new();
            insert.key.hash(&mut hasher);
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
}

struct FlatMap([AtomicU64; 1 << 16]);
