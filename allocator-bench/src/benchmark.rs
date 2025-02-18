use std::io::Write as _;
use std::thread;

use clap::Parser;
use serde::Serialize;

use crate::Backend;
use crate::Barrier;
use crate::Metrics;
use crate::Timer;

mod thread_test;
mod ycsb;

pub trait Interface<B: Backend>: Sync {
    type Global: Sync;
    type Local;

    fn setup_process(
        &self,
        process_count: usize,
        process_id: usize,
        thread_count: usize,
    ) -> Self::Global;

    fn setup_thread(
        &self,
        global: &Self::Global,
        thread_id: usize,
        allocator: &mut B::Allocator,
    ) -> Self::Local;

    fn run_thread(
        &self,
        global: &Self::Global,
        local: &mut Self::Local,
        allocator: &mut B::Allocator,
    );

    fn run_process(
        &self,
        process_count: usize,
        process_id: usize,
        thread_count: usize,
        node: usize,
        name: &str,
        size: usize,
    ) {
        let barrier = &Barrier::new().unwrap();

        // Prevent race conditions between creating and opening shared memory data structures
        let backend = match process_id {
            0 => {
                let backend = B::create(node, name, size);
                barrier.wait(process_count as u64);
                backend
            }
            _ => {
                barrier.wait(process_count as u64);
                B::open(node, name, size)
            }
        };

        let timer = &Timer::new();
        let global = &self.setup_process(process_count, process_id, thread_count);
        let cores = &core_affinity::get_core_ids().unwrap_or_default();

        thread::scope(|scope| {
            let handles = (process_id * thread_count..)
                .take(thread_count)
                .map(|thread_id| {
                    let backend = &backend;
                    let handle = scope.spawn(move || {
                        let core = thread_id % cores.len();
                        core_affinity::set_for_current(cores[core]);

                        let mut allocator = backend.allocator(thread_id);
                        let mut local = self.setup_thread(global, thread_id, &mut allocator);

                        barrier.wait(thread_count as u64);
                        timer.start();
                        self.run_thread(global, &mut local, &mut allocator);
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
                        process_id,
                        thread_id,
                        time,
                    })
                    .unwrap();
                    stdout.write_all(b"\n").unwrap();
                });
        });

        backend.unlink();
    }
}

#[derive(Clone, Parser, Serialize)]
#[serde(tag = "benchmark")]
pub enum Benchmark {
    ThreadTest(thread_test::ThreadTest),
    Ycsb(ycsb::Ycsb),
}

impl Benchmark {
    pub fn args(&self) -> Vec<String> {
        match self {
            Benchmark::ThreadTest(thread_test) => vec![
                "thread-test".to_string(),
                "--iteration-count".to_string(),
                thread_test.iteration_count.to_string(),
                "--object-count".to_string(),
                thread_test.object_count.to_string(),
                "--object-size".to_string(),
                thread_test.object_size.to_string(),
            ],
            Benchmark::Ycsb(_) => vec!["ycsb".to_string()],
        }
    }
}
