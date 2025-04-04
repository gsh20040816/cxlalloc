use core::sync::atomic::Ordering;
use std::time::Instant;

use bon::Builder;
use rand::SeedableRng;
use rand::rngs::SmallRng;
use serde::Deserialize;
use serde::Serialize;
use shm::Shm;

use crate::Index;
use crate::allocator;
use crate::allocator::Backend;
use crate::benchmark;
use crate::config;
use crate::index;

use super::ycsb_load::Field;
use super::ycsb_load::Record;
use super::ycsb_load::insert;
use super::ycsb_load::load;

#[derive(Builder, Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    pub index: index::Config,

    #[serde(flatten)]
    workload: ycsb::Workload,
}

pub struct Global<I> {
    index: I,
    acked: Shm<ycsb::Acknowledged>,
}

unsafe impl<I> Sync for Global<I> {}

#[derive(Deserialize, Serialize)]
pub struct OutputThread {
    operation_count: u64,
    time: u128,
}

pub struct Worker {
    rng: SmallRng,
    operation_count: u64,
}

impl<B: Backend, I: Index<B::Allocator>> benchmark::Benchmark<B> for index::Capture<Config, I> {
    const NAME: &str = "/ycsb";
    type StateGlobal = Global<I>;
    type StateProcess = ();
    type StateCoordinator = ();
    type StateWorker = Worker;

    type OutputWorker = OutputThread;
    type OutputCoordinator = ();

    fn setup_global(
        &self,
        config: &config::Process,
        allocator: &allocator::Config<B::Config>,
    ) -> Self::StateGlobal {
        assert_eq!(
            self.workload.operation_count % config.thread_count,
            0,
            "Operation count {} must be evenly divisible by thread count {}",
            self.workload.operation_count,
            config.thread_count,
        );

        Global {
            index: I::new(
                Some(allocator.numa),
                self.index.len,
                config.is_leader(),
                self.index.populate,
                config.thread_count,
            )
            .unwrap(),
            acked: Shm::builder()
                .name(c"/acked".to_owned())
                .create(config.is_leader())
                .populate(true)
                .build()
                .unwrap(),
        }
    }

    fn setup_process(
        &self,
        _config: &config::Process,
        _allocator: &allocator::Config<B::Config>,
    ) -> Self::StateProcess {
    }

    fn setup_coordinator(
        &self,
        _config: &config::Process,
        _global: &Self::StateGlobal,
        (): &Self::StateProcess,
    ) -> Self::StateCoordinator {
    }

    fn setup_worker(
        &self,
        config: &config::Thread,
        global: &Self::StateGlobal,
        (): &Self::StateProcess,
        allocator: &mut B::Allocator,
    ) -> Self::StateWorker {
        load(&self.workload, config, allocator, &global.index);
        Worker {
            rng: SmallRng::seed_from_u64(config.thread_id as u64),
            operation_count: (self.workload.operation_count / config.thread_count) as u64,
        }
    }

    fn run_coordinator(
        &self,
        _config: &config::Process,
        _global: &Self::StateGlobal,
        (): &Self::StateProcess,
        (): &mut Self::StateCoordinator,
    ) -> Self::OutputCoordinator {
    }

    fn run_worker(
        &self,
        config: &config::Thread,
        global: &Self::StateGlobal,
        (): &Self::StateProcess,
        worker: &mut Self::StateWorker,
        allocator: &mut B::Allocator,
    ) -> Self::OutputWorker {
        let mut runner = self
            .workload
            .runner(unsafe { global.acked.address().as_ref().unwrap() });

        let start = Instant::now();

        for _ in 0..worker.operation_count {
            let operation = runner.next_operation(&mut worker.rng);
            match operation {
                ycsb::Operation::Read => {
                    let key = runner.next_key_read(&mut worker.rng);
                    assert!(global.index.get(
                        config.thread_id,
                        allocator,
                        &key.id().to_ne_bytes(),
                        |value| unsafe {
                            let record = value.cast::<Record>().as_ref().unwrap();
                            for field in &record.0 {
                                (field as *const Field).read_volatile();
                            }
                        },
                    ));
                }
                ycsb::Operation::Update => {
                    let key = runner.next_key_read(&mut worker.rng);
                    let field = runner.next_field(&mut worker.rng);
                    assert!(global.index.get(
                        config.thread_id,
                        allocator,
                        &key.id().to_ne_bytes(),
                        |value| unsafe {
                            let record = value.cast::<Record>().as_ref().unwrap();
                            record.0[field as usize].value[0].store(1, Ordering::Release);
                        },
                    ));
                }
                ycsb::Operation::Scan => todo!(),
                ycsb::Operation::Insert => {
                    let key = runner.next_key_insert(&mut worker.rng);
                    insert(config.thread_id, allocator, &global.index, &key);
                    runner.acknowledge(key);
                }
                ycsb::Operation::ReadModifyWrite => todo!(),
                ycsb::Operation::Delete => todo!(),
            }
        }
        let time = start.elapsed();

        OutputThread {
            operation_count: worker.operation_count,
            time: time.as_nanos(),
        }
    }

    fn teardown_global(&self, config: &config::Process, mut global: Self::StateGlobal) {
        if !config.is_leader() {
            return;
        }

        global.index.unlink().unwrap();
        global.acked.unlink().unwrap();
    }
}
