use core::hash::Hash;
use core::hash::Hasher as _;
use core::mem;
use core::sync::atomic::AtomicU8;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::Ordering;
use std::path::PathBuf;

use clap::Parser;
use rapidhash::RapidHasher;
use serde::Serialize;
use shm::Shm;

use crate::Allocator;
use crate::Backend;
use crate::Pointer;
use crate::benchmark;

#[derive(Clone, Parser, Serialize)]
pub struct Ycsb {
    workload: PathBuf,
    #[arg(long)]
    load: bool,
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
    index: Shm<FlatMap>,
}

#[repr(C)]
struct Record {
    key: AtomicU64,
    fields: [Field; 10],
}

const _: () = assert!(mem::size_of::<Record>() == 968);

#[repr(C)]
struct Field {
    value: [AtomicU8; 96],
}

unsafe impl Sync for Global {}

impl<B: Backend> benchmark::Interface<B> for Ycsb {
    const NAME: &str = "ycsb";
    type Global = Global;
    type Local = ();

    fn setup_process(
        &self,
        numa: usize,
        _process_count: usize,
        _process_id: usize,
        _thread_count: usize,
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
            index: Shm::new(Some(numa), c"index".to_owned()).unwrap(),
        }
    }

    fn setup_thread(
        &self,
        process_count: usize,
        _process_id: usize,
        thread_count: usize,
        thread_id: usize,
        global: &Self::Global,
        allocator: &mut B::Allocator,
    ) -> Self::Local {
        if self.load {
            return;
        }

        let map = unsafe { global.index.address().as_ref() }.unwrap();
        let thread_total = process_count * thread_count;
        let mut loader = global.workload.loader(thread_total, thread_id);

        for _ in 0..global.workload.record_count() / thread_total {
            let key = loader.next_key();
            // FIXME: CXL-SHM max record size
            let handle = allocator.allocate(mem::size_of::<Record>()).unwrap();
            let offset = unsafe { allocator.pointer_to_offset(&handle) };
            unsafe {
                handle
                    .as_ptr()
                    .cast::<Record>()
                    .as_ref()
                    .unwrap()
                    .key
                    .store(key, Ordering::Release);
            }

            map.insert(key, offset);
        }
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

        if self.load {
            let mut loader = global.workload.loader(thread_total, thread_id);

            for _ in 0..global.workload.record_count() / thread_total {
                let key = loader.next_key();
                // FIXME: CXL-SHM max record size
                let handle = allocator.allocate(mem::size_of::<Record>()).unwrap();
                let offset = unsafe { allocator.pointer_to_offset(&handle) };
                unsafe {
                    handle
                        .as_ptr()
                        .cast::<Record>()
                        .as_ref()
                        .unwrap()
                        .key
                        .store(key, Ordering::Release);
                }
                map.insert(key, offset);
            }
        } else {
            let mut runner = global.workload.runner();
            let mut rng = rand::rng();
            for _ in 0..global.workload.operation_count() / thread_total {
                let key = runner.next_key(&mut rng);
                match runner.next_operation(&mut rng) {
                    ycsb::Operation::Read => {
                        let fields = map
                            .get(key, |offset| {
                                let record = unsafe {
                                    allocator
                                        .offset_to_pointer(offset)?
                                        .as_ptr()
                                        .cast::<Record>()
                                        .as_ref()?
                                };

                                if record.key.load(Ordering::Acquire) != key {
                                    return None;
                                }

                                Some(&record.fields)
                            })
                            .unwrap();

                        for field in fields {
                            unsafe {
                                (field as *const Field).read_volatile();
                            }
                        }
                    }
                    ycsb::Operation::Update => {
                        let field = runner.next_field(&mut rng);
                        map.get(key, |offset| {
                            let record = unsafe {
                                allocator
                                    .offset_to_pointer(offset)?
                                    .as_ptr()
                                    .cast::<Record>()
                                    .as_ref()?
                            };

                            if record.key.load(Ordering::Acquire) != key {
                                return None;
                            }

                            record.fields[field as usize].value[0].store(1, Ordering::Release);
                            Some(())
                        })
                        .unwrap();
                    }
                    ycsb::Operation::Scan => todo!(),
                    ycsb::Operation::Insert => todo!(),
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

impl FlatMap {
    const MAX_PROBE: usize = 16;

    pub fn insert<K: Hash>(&self, key: K, value: u64) {
        let index = self.index(key);
        let mut probe = 0;

        loop {
            while self.0[(index + probe) % self.0.len()].load(Ordering::Acquire) > 0 {
                probe += 1;
            }

            if self.0[(index + probe) % self.0.len()]
                .compare_exchange(0, value + 1, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                assert!(
                    probe < Self::MAX_PROBE,
                    "Expected probe = {} < {}",
                    probe,
                    Self::MAX_PROBE
                );
                break;
            }
        }
    }

    pub fn get<K: Hash, F: FnMut(u64) -> Option<T>, T>(&self, key: K, mut compare: F) -> Option<T> {
        let index = self.index(key);

        for probe in 0..Self::MAX_PROBE {
            match self.0[(index + probe) % self.0.len()].load(Ordering::Acquire) {
                0 => continue,
                offset => match compare(offset - 1) {
                    None => continue,
                    value @ Some(_) => return value,
                },
            }
        }

        None
    }

    fn index<K: Hash>(&self, key: K) -> usize {
        let mut hasher = RapidHasher::default();
        key.hash(&mut hasher);
        hasher.finish() as usize % self.0.len()
    }
}
