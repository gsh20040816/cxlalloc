// https://github.com/emeryberger/Hoard/blob/f021bdb810332c9c9f5a11ae5404aaa38fe129c0/benchmarks/threadtest/threadtest.cpp

use clap::Parser;

use crate::Allocator;
use crate::Backend;
use crate::benchmark;

#[derive(Clone, Parser)]
pub struct ThreadTest {
    #[arg(short, long, default_value_t = 50)]
    pub(crate) iteration_count: usize,

    #[arg(short = 'n', long, default_value_t = 30_000)]
    pub(crate) object_count: usize,

    #[arg(short = 's', long, default_value_t = 8)]
    pub(crate) object_size: usize,
}

impl<B: Backend> benchmark::Interface<B> for ThreadTest {
    type Global = usize;
    type Local = Vec<Option<<B::Allocator as Allocator>::Ptr>>;

    fn setup_process(&self, process_count: usize, _: usize, thread_count: usize) -> Self::Global {
        assert_eq!(
            self.object_count % (process_count * thread_count),
            0,
            "Object count should be multiple of total thread count"
        );

        self.object_count / (process_count * thread_count)
    }

    fn setup_thread(&self, object_count: &Self::Global, _: usize) -> Self::Local {
        (0..*object_count).map(|_| None).collect()
    }

    fn run_thread(&self, _: &Self::Global, mut pointers: Self::Local, mut allocator: B::Allocator) {
        for _ in 0..self.iteration_count {
            for pointer in &mut pointers {
                *pointer = allocator.allocate(self.object_size);
            }

            for pointer in &mut pointers {
                let pointer = pointer.take().unwrap();
                unsafe {
                    allocator.deallocate(pointer);
                }
            }
        }
    }
}
