use std::env;
use std::thread;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use serde::Deserialize;
use serde::Serialize;

use crate::Barrier;
use crate::Output;
use crate::Perf;
use crate::ResourceUsage;
use crate::allocator;
use crate::allocator::Backend;
use crate::config;

pub mod memcached;
mod mstress;
mod thread_test;
mod xmalloc;
pub mod ycsb;
pub mod ycsb_load;

pub use memcached::Memcached;
pub use mstress::Mstress;
pub use thread_test::ThreadTest;
pub use xmalloc::Xmalloc;
pub use ycsb::Ycsb;
pub use ycsb_load::YcsbLoad;

pub trait Benchmark<B: Backend>: Sync {
    const NAME: &str;

    type StateGlobal: Sync;
    type StateCoordinator;
    type StateWorker;

    type OutputGlobal: Serialize;
    type OutputWorker: Send;
    type OutputCoordinator: Send;

    fn setup_process(
        &self,
        config: &config::Process,
        allocator: &allocator::Config,
    ) -> Self::StateGlobal;

    fn setup_coordinator(
        &self,
        config: &config::Process,
        global: &Self::StateGlobal,
    ) -> Self::StateCoordinator;

    fn setup_worker(
        &self,
        config: &config::Thread,
        global: &Self::StateGlobal,
        allocator: &mut B::Allocator,
    ) -> Self::StateWorker;

    fn run_coordinator(
        &self,
        _config: &config::Process,
        _global: &Self::StateGlobal,
        _coordinator: &mut Self::StateCoordinator,
    ) -> Self::OutputCoordinator;

    fn run_worker(
        &self,
        config: &config::Thread,
        global: &Self::StateGlobal,
        worker: &mut Self::StateWorker,
        allocator: &mut B::Allocator,
    ) -> Self::OutputWorker;

    fn teardown_process(&self, _config: &config::Process, _global: Self::StateGlobal) {}

    fn aggregate(
        coordinator: Self::OutputCoordinator,
        workers: Vec<Self::OutputWorker>,
    ) -> Self::OutputGlobal;

    fn run_process(&self, config: &config::Process, allocator: &allocator::Config) {
        let thread_count = config.thread_count as u64 + 1;
        let thread_total = config.process_count as u64 * thread_count;

        let mut barrier = Barrier::new(thread_total).unwrap();

        // Prevent race conditions between creating and opening shared memory data structures
        let backend = match config.process_id {
            0 => {
                let backend = B::create(allocator, Self::NAME);
                barrier.wait(thread_count);
                backend
            }
            _ => {
                barrier.wait(thread_count);
                B::open(allocator, Self::NAME)
            }
        }
        .unwrap();

        let global = self.setup_process(config, allocator);
        let cores = &core_affinity::get_core_ids().unwrap_or_default();
        let date = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut perf = match (
            config.process_id,
            env::var("PERF_CTL_FIFO"),
            env::var("PERF_ACK_FIFO"),
        ) {
            (0, Ok(ctl), Ok(ack)) => Some(Perf::new(ctl, ack)),
            _ => None,
        };

        thread::scope(|scope| {
            let workers = (config.process_id * config.thread_count..)
                .take(config.thread_count)
                .map(|thread_id| {
                    let barrier = &barrier;
                    let backend = &backend;
                    let global = &global;
                    let handle = scope.spawn(move || {
                        let config = config::Thread {
                            process: *config,
                            thread_id,
                        };
                        let core = thread_id % cores.len();
                        core_affinity::set_for_current(cores[core]);

                        let mut allocator = backend.allocator(thread_id);
                        let mut worker = self.setup_worker(&config, global, &mut allocator);

                        barrier.wait(1);
                        let data = self.run_worker(&config, global, &mut worker, &mut allocator);
                        barrier.wait(1);

                        drop(allocator);
                        drop(worker);
                        data
                    });
                    handle
                })
                .collect::<Vec<_>>();

            let coordinator = scope.spawn(|| {
                let mut coordinator = self.setup_coordinator(config, &global);

                if let Some(perf) = &mut perf {
                    perf.enable();
                }

                let before = ResourceUsage::new().unwrap();
                barrier.wait(1);
                let output = self.run_coordinator(config, &global, &mut coordinator);
                barrier.wait(1);
                let after = ResourceUsage::new().unwrap();

                if let Some(perf) = &mut perf {
                    perf.disable();
                }

                (before, output, after)
            });

            let output_workers = workers
                .into_iter()
                .map(|handle| handle.join())
                .collect::<Result<Vec<_>, _>>()
                .unwrap();

            let (before, output_coordinator, after) = coordinator.join().unwrap();

            let output = Self::aggregate(output_coordinator, output_workers);

            let mut stdout = std::io::stdout().lock();
            serde_json::ser::to_writer(&mut stdout, &Output {
                date,
                resource_usage: after - before,
                process_id: config.process_id,
                data: serde_json::to_value(output).unwrap(),
            })
            .unwrap();
        });

        self.teardown_process(config, global);

        if config.process_id == 0 {
            barrier.unlink().unwrap();
            backend.unlink().unwrap();
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case", tag = "name")]
pub enum Config {
    Mstress(Mstress),
    Memcached(memcached::Config),
    ThreadTest(thread_test::ThreadTest),
    Ycsb(ycsb::Config),
    YcsbLoad(ycsb_load::Config),
    Xmalloc(xmalloc::Xmalloc),
}
