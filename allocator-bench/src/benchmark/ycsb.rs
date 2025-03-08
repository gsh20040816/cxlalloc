use core::mem;
use core::sync::atomic::AtomicU8;
use core::sync::atomic::Ordering;

use serde::Deserialize;
use serde::Serialize;
use shm::Shm;

use crate::Backend;
use crate::Index;
use crate::benchmark;
use crate::context;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Ycsb {
    pub load: bool,

    #[serde(flatten)]
    pub workload: ycsb::Workload,
}

// HACK: CXL-SHM doesn't support allocations larger than 1KiB (1_000B data + 24B header)
#[expect(unused)]
const MAX_SIZE: usize = 1_000;

pub struct Global<I> {
    index: I,
    acked: Shm<ycsb::Acknowledged>,
}

struct Record([Field; 10]);

#[repr(C)]
struct Field {
    value: [AtomicU8; 96],
}

unsafe impl<I> Sync for Global<I> {}

impl<B: Backend, I: Index<B::Allocator>> benchmark::Interface<B, I> for Ycsb {
    const NAME: &str = "ycsb";
    type Global = Global<I>;
    type Local = ();

    fn setup_process(&self, context: &context::Process) -> Global<I> {
        Global {
            index: I::new(Some(context.numa), "index", 1 << 24, true).unwrap(),
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

        let mut loader = self
            .workload
            .loader(context.thread_total(), context.thread_id);
        while let Some(key) = loader.next_key() {
            global
                .index
                .insert(allocator, key.id(), mem::size_of::<Record>(), |_| ());
        }
    }

    fn run_thread(
        &self,
        context: &context::Thread,
        global: &Self::Global,
        _local: &mut Self::Local,
        allocator: &mut B::Allocator,
    ) {
        if self.load {
            let mut loader = self
                .workload
                .loader(context.thread_total(), context.thread_id);
            while let Some(key) = loader.next_key() {
                global
                    .index
                    .insert(allocator, key.id(), mem::size_of::<Record>(), |_| ());
            }
            return;
        }

        let mut runner = self
            .workload
            .runner(unsafe { global.acked.address().as_ref().unwrap() });
        let mut rng = rand::rng();
        for _ in 0..self.workload.operation_count() / context.thread_total() {
            let key = runner.next_key(&mut rng);
            let id = key.id();
            match runner.next_operation(&mut rng) {
                ycsb::Operation::Read => unsafe {
                    let found = global.index.get(allocator, id, |value| {
                        let record = value.cast::<Record>().as_ref().unwrap();
                        for field in &record.0 {
                            (field as *const Field).read_volatile();
                        }
                    });

                    assert!(found);
                },
                ycsb::Operation::Update => {
                    let field = runner.next_field(&mut rng);

                    let found = global.index.get(allocator, id, |value| unsafe {
                        let record = value.cast::<Record>().as_ref().unwrap();
                        record.0[field as usize].value[0].store(1, Ordering::Release);
                    });

                    assert!(found);
                }
                ycsb::Operation::Scan => todo!(),
                ycsb::Operation::Insert => {
                    global
                        .index
                        .insert(allocator, key.id(), mem::size_of::<Record>(), |_| ());
                }
                ycsb::Operation::ReadModifyWrite => todo!(),
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
