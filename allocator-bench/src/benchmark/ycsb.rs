use core::mem;
use core::sync::atomic::AtomicU8;
use core::sync::atomic::Ordering;

use serde::Deserialize;
use serde::Serialize;
use shm::Shm;

use crate::Allocator;
use crate::Index;
use crate::allocator::Backend;
use crate::allocator::Handle as _;
use crate::benchmark;
use crate::context;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Ycsb {
    /// Whether to measure loading only (or else running phase only)
    pub load: bool,

    /// Whether to inline the value into index entries (or else allocate separately)
    pub index_inline: bool,

    /// Size of hash map backing array
    pub index_len: usize,

    /// Whether to map populate the index
    pub index_populate: bool,

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
            index: I::new(
                Some(context.allocator_numa),
                "index",
                self.index_len,
                self.index_populate,
            )
            .unwrap(),
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

        match self.index_inline {
            true => load::<true, _, _>(&self.workload, context, allocator, &global.index),
            false => load::<false, _, _>(&self.workload, context, allocator, &global.index),
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
            match self.index_inline {
                true => load::<true, _, _>(&self.workload, context, allocator, &global.index),
                false => load::<false, _, _>(&self.workload, context, allocator, &global.index),
            }

            return;
        }

        let mut runner = self
            .workload
            .runner(unsafe { global.acked.address().as_ref().unwrap() });

        match self.index_inline {
            true => run::<true, _, _>(
                self.workload.operation_count(),
                context,
                &mut runner,
                allocator,
                &global.index,
            ),
            false => run::<false, _, _>(
                self.workload.operation_count(),
                context,
                &mut runner,
                allocator,
                &global.index,
            ),
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

fn load<const INLINE: bool, A: Allocator, I: Index<A>>(
    workload: &ycsb::Workload,
    context: &context::Thread,
    allocator: &mut A,
    index: &I,
) {
    let mut loader = workload.loader(context.thread_total(), context.thread_id);

    while let Some(key) = loader.next_key() {
        insert::<INLINE, _, _>(allocator, index, &key);
    }
}

fn run<const INLINE: bool, A: Allocator, I: Index<A>>(
    operation_count: usize,
    context: &context::Thread,
    runner: &mut ycsb::Runner,
    allocator: &mut A,
    index: &I,
) {
    let mut rng = rand::rng();

    for _ in 0..operation_count / context.thread_total() {
        let key = runner.next_key(&mut rng);
        match runner.next_operation(&mut rng) {
            ycsb::Operation::Read => {
                with::<INLINE, _, _, _>(allocator, index, &key, |value| unsafe {
                    let record = value.cast::<Record>().as_ref().unwrap();
                    for field in &record.0 {
                        (field as *const Field).read_volatile();
                    }
                })
            }
            ycsb::Operation::Update => {
                let field = runner.next_field(&mut rng);
                with::<INLINE, _, _, _>(allocator, index, &key, |value| unsafe {
                    let record = value.cast::<Record>().as_ref().unwrap();
                    record.0[field as usize].value[0].store(1, Ordering::Release);
                });
            }
            ycsb::Operation::Scan => todo!(),
            ycsb::Operation::Insert => {
                insert::<INLINE, _, _>(allocator, index, &key);
            }
            ycsb::Operation::ReadModifyWrite => todo!(),
        }
    }
}

fn insert<const INLINE: bool, A: Allocator, I: Index<A>>(
    allocator: &mut A,
    index: &I,
    key: &ycsb::Key,
) {
    match INLINE {
        true => index.insert(allocator, key.id(), mem::size_of::<Record>(), |_, _| ()),
        false => {
            let value = allocator.allocate(mem::size_of::<Record>()).unwrap();
            index.insert(
                allocator,
                key.id(),
                mem::size_of::<u64>(),
                |allocator, pointer| unsafe {
                    allocator.link(pointer.cast(), &value);
                },
            );
        }
    }
}

fn with<const INLINE: bool, A: Allocator, I: Index<A>, F: FnOnce(*const u8)>(
    allocator: &mut A,
    index: &I,
    key: &ycsb::Key,
    with: F,
) {
    match INLINE {
        true => {
            let found = index.get(allocator, key.id(), |_, value| with(value));
            assert!(found);
        }
        false => {
            let found = index.get(allocator, key.id(), |allocator, pointer| {
                let offset = unsafe { pointer.cast::<u64>().read() };
                let handle = allocator.offset_to_handle(offset).unwrap();
                with(handle.as_ptr().cast())
            });
            assert!(found);
        }
    }
}
