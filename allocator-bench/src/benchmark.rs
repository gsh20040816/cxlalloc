use std::thread;

use clap::Parser;

use crate::Backend;
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

    fn setup_thread(&self, global: &Self::Global, thread_id: usize) -> Self::Local;

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
        name: &str,
        size: usize,
    ) {
        let backend = &B::open(name, size);
        let timer = &Timer::new();
        let global = &self.setup_process(process_count, process_id, thread_count);

        thread::scope(|scope| {
            let handles = (process_id * thread_count..)
                .take(thread_count)
                .map(|thread_id| {
                    scope.spawn(move || {
                        let mut allocator = backend.allocator(thread_id);
                        let mut local = self.setup_thread(global, thread_id);
                        timer.start();
                        self.run_thread(global, &mut local, &mut allocator);
                        timer.stop(thread_id);
                        drop(allocator);
                        drop(local);
                    })
                })
                .collect::<Vec<_>>();

            handles
                .into_iter()
                .map(|handle| handle.join())
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
        });
    }
}

#[derive(Clone, Parser)]
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
