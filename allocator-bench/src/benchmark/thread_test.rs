// https://github.com/emeryberger/Hoard/blob/f021bdb810332c9c9f5a11ae5404aaa38fe129c0/benchmarks/threadtest/threadtest.cpp

use bon::Builder;
use serde::Deserialize;
use serde::Serialize;

use crate::Allocator;
use crate::Backend;
use crate::benchmark;
use crate::context;

#[derive(Builder, Clone, Deserialize, Serialize)]
pub struct ThreadTest {
    #[builder(default = 50)]
    pub(crate) iteration_count: usize,

    #[builder(default = 30_000)]
    pub(crate) object_count: usize,

    #[builder(default = 8)]
    pub(crate) object_size: usize,
}

impl<B: Backend> benchmark::Interface<B> for ThreadTest {
    const NAME: &str = "tt";
    type Global = usize;
    type Local = Vec<Option<<B::Allocator as Allocator>::Ptr>>;

    fn setup_process(&self, context: &context::Process) -> Self::Global {
        let thread_total = context.thread_total();
        assert_eq!(
            self.object_count % thread_total,
            0,
            "Object count should be multiple of total thread count"
        );

        self.object_count / thread_total
    }

    fn setup_thread(
        &self,
        _context: &context::Thread,
        object_count: &Self::Global,
        _allocator: &mut B::Allocator,
    ) -> Self::Local {
        (0..*object_count).map(|_| None).collect()
    }

    fn run_thread(
        &self,
        _context: &context::Thread,
        _: &Self::Global,
        pointers: &mut Self::Local,
        allocator: &mut B::Allocator,
    ) {
        for _ in 0..self.iteration_count {
            for pointer in &mut *pointers {
                *pointer = allocator.allocate(self.object_size);
            }

            for pointer in &mut *pointers {
                let pointer = pointer.take().unwrap();
                unsafe {
                    allocator.deallocate(pointer);
                }
            }
        }
    }
}
