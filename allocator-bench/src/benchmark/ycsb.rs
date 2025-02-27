use core::mem;
use core::sync::atomic::AtomicU8;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::Ordering;
use std::path::PathBuf;

use clap::Parser;
use serde::Serialize;
use shm::Shm;

use crate::Allocator;
use crate::Backend;
use crate::Pointer;
use crate::benchmark;
use crate::context;
use crate::index::LinearHashMap;

#[derive(Clone, Parser, Serialize)]
pub struct Ycsb {
    workload: PathBuf,
    #[arg(long)]
    load: bool,
}

// HACK: CXL-SHM doesn't support allocations larger than 1KiB (1_000B data + 24B header)
#[expect(unused)]
const MAX_SIZE: usize = 1_000;

pub struct Global {
    workload: ycsb::Workload,
    index: LinearHashMap,
    acked: Shm<ycsb::Acknowledged>,
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

    fn setup_process(&self, context: &context::Process) -> Self::Global {
        let workload = std::fs::read_to_string(&self.workload).unwrap_or_else(|error| {
            panic!(
                "Failed to read workload file {:?}: {:?}",
                self.workload, error
            )
        });
        let workload = toml::from_str(&workload).unwrap();

        Global {
            workload,
            index: LinearHashMap::new(Some(context.numa), "index", 1 << 24, true).unwrap(),
            acked: Shm::new(None, c"acked".to_owned(), true).unwrap(),
        }
    }

    fn setup_thread(
        &self,
        context: &context::Thread,
        global: &Self::Global,
        allocator: &mut B::Allocator,
    ) -> Self::Local {
        if self.load {
            return;
        }

        load::<B>(context, global, allocator);
    }

    fn run_thread(
        &self,
        context: &context::Thread,
        global: &Self::Global,
        _local: &mut Self::Local,
        allocator: &mut B::Allocator,
    ) {
        if self.load {
            load::<B>(context, global, allocator);
        } else {
            let mut runner = global
                .workload
                .runner(unsafe { global.acked.address().as_ref().unwrap() });
            let mut rng = rand::rng();
            for _ in 0..global.workload.operation_count() / context.thread_total() {
                let key = runner.next_key(&mut rng);
                let id = key.id();
                match runner.next_operation(&mut rng) {
                    ycsb::Operation::Read => {
                        let fields = global
                            .index
                            .get(id, |offset| {
                                let record = unsafe {
                                    allocator
                                        .offset_to_pointer(offset)?
                                        .as_ptr()
                                        .cast::<Record>()
                                        .as_ref()?
                                };

                                if record.key.load(Ordering::Acquire) != id {
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
                        global
                            .index
                            .get(id, |offset| {
                                let record = unsafe {
                                    allocator
                                        .offset_to_pointer(offset)?
                                        .as_ptr()
                                        .cast::<Record>()
                                        .as_ref()?
                                };

                                if record.key.load(Ordering::Acquire) != id {
                                    return None;
                                }

                                record.fields[field as usize].value[0].store(1, Ordering::Release);
                                Some(())
                            })
                            .unwrap();
                    }
                    ycsb::Operation::Scan => todo!(),
                    ycsb::Operation::Insert => {
                        insert::<B>(allocator, &global.index, key);
                    }
                    ycsb::Operation::ReadModifyWrite => todo!(),
                }
            }
        }
    }

    fn teardown_process(&self, context: &context::Process, mut global: Self::Global) {
        if context.process_id != 0 {
            return;
        }

        global.index.unlink().unwrap();
        global.acked.unlink().unwrap();
    }
}

fn load<B: Backend>(context: &context::Thread, global: &Global, allocator: &mut B::Allocator) {
    let mut loader = global
        .workload
        .loader(context.thread_total(), context.thread_id);
    while let Some(key) = loader.next_key() {
        insert::<B>(allocator, &global.index, key);
    }
}

fn insert<B: Backend>(allocator: &mut B::Allocator, map: &LinearHashMap, key: ycsb::Key) {
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
            .store(key.id(), Ordering::Release);
    }
    map.insert(key.id(), offset);
}
