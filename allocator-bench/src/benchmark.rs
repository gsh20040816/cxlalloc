use core::iter;
use std::env;
use std::io::Write as _;
use std::thread;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use serde::Deserialize;
use serde::Serialize;

use crate::Barrier;
use crate::Index;
use crate::Metrics;
use crate::Perf;
use crate::allocator;
use crate::allocator::Backend;
use crate::config;

mod thread_test;
mod xmalloc;
pub mod ycsb;

pub use thread_test::ThreadTest;
pub use xmalloc::Xmalloc;

pub trait Benchmark<B: Backend, I: Index<B::Allocator>>: Sync {
    const NAME: &str;

    type Global: Sync;
    type Local;

    type Data: Send + Serialize;

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

    fn run_coordinator(&self, _config: &config::Process, _global: &Self::Global) {}

    fn run_thread(
        &self,
        config: &config::Thread,
        global: &Self::Global,
        local: &mut Self::Local,
        allocator: &mut B::Allocator,
    ) -> Self::Data;

    fn teardown_process(&self, _config: &config::Process, _global: Self::Global) {}

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

        let mut perf = match (env::var("PERF_CTL_FIFO"), env::var("PERF_ACK_FIFO")) {
            (Ok(ctl), Ok(ack)) => Some(Perf::new(ctl, ack)),
            _ => None,
        };

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

                        barrier.wait(1);
                        let data = self.run_thread(&config, global, &mut local, &mut allocator);
                        barrier.wait(1);

                        drop(allocator);
                        drop(local);

                        Some(data)
                    });
                    (thread_id, handle)
                })
                .chain({
                    let thread_id = config.thread_total();
                    let handle = scope.spawn(|| {
                        if let Some(perf) = &mut perf {
                            perf.enable();
                        }

                        barrier.wait(1);
                        self.run_coordinator(config, &global);
                        barrier.wait(1);

                        if let Some(perf) = &mut perf {
                            perf.disable();
                        }

                        None
                    });
                    iter::once((thread_id, handle))
                })
                .collect::<Vec<_>>();

            handles
                .into_iter()
                .map(|(thread_id, handle)| handle.join().map(|data| (thread_id, data)))
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
                .into_iter()
                .filter_map(|(thread_id, data)| Some((thread_id, data?)))
                .for_each(|(thread_id, data)| {
                    let mut stdout = std::io::stdout().lock();
                    serde_json::ser::to_writer(&mut stdout, &Metrics {
                        date,
                        process_id: config.process_id,
                        thread_id,
                        data: serde_json::to_value(data).unwrap(),
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
#[serde(rename_all = "snake_case", tag = "name")]
pub enum Config {
    ThreadTest(thread_test::ThreadTest),
    Ycsb(ycsb::Ycsb),
    Xmalloc(xmalloc::Xmalloc),
}
