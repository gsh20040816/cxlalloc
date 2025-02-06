use std::thread;

use clap::Parser;

use crate::Backend;

mod thread_test;

pub(crate) trait Interface<B: Backend>: Sync {
    type Global: Sync;
    type Local;

    fn setup_process(
        &self,
        process_count: usize,
        process_id: usize,
        thread_count: usize,
    ) -> Self::Global;

    fn setup_thread(&self, global: &Self::Global, thread_id: usize) -> Self::Local;

    fn run_thread(&self, global: &Self::Global, local: Self::Local, allocator: B::Allocator);

    fn run_process(
        &self,
        process_count: usize,
        process_id: usize,
        thread_count: usize,
        name: &str,
        size: usize,
    ) {
        let mut backend = B::open(name, size);

        let global = &self.setup_process(process_count, process_id, thread_count);

        thread::scope(|scope| {
            let handles = (process_id * thread_count..)
                .take(thread_count)
                .map(|thread_id| {
                    let allocator = backend.allocator(thread_id);
                    scope.spawn(move || {
                        let local = self.setup_thread(global, thread_id);
                        self.run_thread(global, local, allocator)
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
pub(crate) enum Benchmark {
    ThreadTest(thread_test::ThreadTest),
}
