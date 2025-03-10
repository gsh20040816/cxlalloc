use std::env;
use std::io::Write as _;
use std::thread;

use serde::Deserialize;
use serde::Serialize;

use crate::Barrier;
use crate::Index;
use crate::Metrics;
use crate::Perf;
use crate::Timer;
use crate::allocator;
use crate::allocator::Backend;
use crate::config;

mod thread_test;
pub mod ycsb;

pub trait Benchmark<B: Backend, I: Index<B::Allocator>>: Sync {
    const NAME: &str;

    type Global: Sync;
    type Local;

    fn setup_process(
        &self,
        config: &config::Process,
        allocator: &allocator::Config,
    ) -> Self::Global;

    fn setup_thread(
        &self,
        config: &config::Thread,
        global: &Self::Global,
        allocator: &mut B::Allocator,
    ) -> Self::Local;

    fn run_thread(
        &self,
        config: &config::Thread,
        global: &Self::Global,
        local: &mut Self::Local,
        allocator: &mut B::Allocator,
    );

    fn teardown_process(&self, _config: &config::Process, _global: Self::Global) {}

    fn run_process(&self, config: &config::Process, allocator: &allocator::Config) {
        let mut barrier = Barrier::new().unwrap();

        // Prevent race conditions between creating and opening shared memory data structures
        let backend = match config.process_id {
            0 => {
                let backend = B::open(allocator, Self::NAME);
                barrier.wait(config.thread_total() as u64, config.thread_count as u64);
                backend
            }
            _ => {
                barrier.wait(config.thread_total() as u64, config.thread_count as u64);
                B::open(allocator, Self::NAME)
            }
        }
        .unwrap();

        let timer = &Timer::new();
        let global = self.setup_process(config, allocator);
        let cores = &core_affinity::get_core_ids().unwrap_or_default();

        thread::scope(|scope| {
            let handles = (config.process_id * config.thread_count..)
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
                        let mut local = self.setup_thread(&config, global, &mut allocator);

                        let mut perf = match (
                            thread_id,
                            env::var("PERF_CTL_FIFO"),
                            env::var("PERF_ACK_FIFO"),
                        ) {
                            (0, Ok(ctl), Ok(ack)) => Some(Perf::new(ctl, ack)),
                            _ => None,
                        };

                        if let Some(perf) = &mut perf {
                            perf.enable();
                        }

                        barrier.wait(config.thread_total() as u64, 1);
                        timer.start();
                        self.run_thread(&config, global, &mut local, &mut allocator);
                        let time = timer.stop();

                        if let Some(perf) = &mut perf {
                            perf.disable();
                        }

                        drop(allocator);
                        drop(local);

                        time
                    });
                    (thread_id, handle)
                })
                .collect::<Vec<_>>();

            handles
                .into_iter()
                .map(|(thread_id, handle)| handle.join().map(|output| (thread_id, output)))
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
                .into_iter()
                .for_each(|(thread_id, time)| {
                    let mut stdout = std::io::stdout().lock();
                    serde_json::ser::to_writer(&mut stdout, &Metrics {
                        process_id: config.process_id,
                        thread_id,
                        time,
                    })
                    .unwrap();
                    stdout.write_all(b"\n").unwrap();
                });
        });

        self.teardown_process(config, global);

        if config.process_id == 0 {
            barrier.unlink().unwrap();
            backend.unlink().unwrap();
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Config {
    ThreadTest(thread_test::ThreadTest),
    Ycsb(ycsb::Ycsb),
}
