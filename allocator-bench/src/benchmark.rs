use std::io::Write as _;
use std::thread;

use clap::Parser;
use serde::Deserialize;
use serde::Serialize;

use crate::Backend;
use crate::Barrier;
use crate::Metrics;
use crate::Timer;
use crate::context;

mod thread_test;
mod ycsb;

pub trait Interface<B: Backend>: Sync {
    const NAME: &str;

    type Global: Sync;
    type Local;

    fn setup_process(&self, context: &context::Process) -> Self::Global;

    fn setup_thread(
        &self,
        context: &context::Thread,
        global: &Self::Global,
        allocator: &mut B::Allocator,
    ) -> Self::Local;

    fn run_thread(
        &self,
        context: &context::Thread,
        global: &Self::Global,
        local: &mut Self::Local,
        allocator: &mut B::Allocator,
    );

    fn teardown_process(&self, _context: &context::Process, _global: Self::Global) {}

    fn run_process(&self, context: &context::Process, size: usize) {
        let mut barrier = Barrier::new().unwrap();

        // Prevent race conditions between creating and opening shared memory data structures
        let backend = match context.process_id {
            0 => {
                let backend = B::open(context.numa, context.populate, Self::NAME, size);
                barrier.wait(context.thread_total() as u64, context.thread_count as u64);
                backend
            }
            _ => {
                barrier.wait(context.thread_total() as u64, context.thread_count as u64);
                B::open(context.numa, context.populate, Self::NAME, size)
            }
        }
        .unwrap();

        let timer = &Timer::new();
        let global = self.setup_process(context);
        let cores = &core_affinity::get_core_ids().unwrap_or_default();

        thread::scope(|scope| {
            let handles = (context.process_id * context.thread_count..)
                .take(context.thread_count)
                .map(|thread_id| {
                    let barrier = &barrier;
                    let backend = &backend;
                    let global = &global;
                    let handle = scope.spawn(move || {
                        let context = context::Thread {
                            process: *context,
                            thread_id,
                        };
                        let core = thread_id % cores.len();
                        core_affinity::set_for_current(cores[core]);

                        let mut allocator = backend.allocator(thread_id);
                        let mut local = self.setup_thread(&context, global, &mut allocator);

                        barrier.wait(context.thread_total() as u64, 1);
                        timer.start();
                        self.run_thread(&context, global, &mut local, &mut allocator);
                        let time = timer.stop();

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
                        process_id: context.process_id,
                        thread_id,
                        time,
                    })
                    .unwrap();
                    stdout.write_all(b"\n").unwrap();
                });
        });

        self.teardown_process(context, global);

        if context.process_id == 0 {
            barrier.unlink().unwrap();
            backend.unlink().unwrap();
        }
    }
}

#[derive(Clone, Parser, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Benchmark {
    ThreadTest(thread_test::ThreadTest),
    Ycsb(ycsb::Ycsb),
}
