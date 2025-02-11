use clap::Parser;

use crate::Allocator;
use crate::Backend;
use crate::benchmark;

#[derive(Parser)]
pub enum Ycsb {
    Load,
}

// #[repr(C)]
// pub struct Insert {
//     key: [u8; 23]
// }
//
// #[repr(C)]
// struct Field {
//     name: [u8; 6],
//     value:
// }

impl<B: Backend> benchmark::Interface<B> for Ycsb {
    type Global = ;
    type Local = Vec<Command>;

    fn setup_process(
        &self,
        process_count: usize,
        process_id: usize,
        thread_count: usize,
    ) -> Self::Global {
        todo!()
    }

    fn setup_thread(&self, global: &Self::Global, thread_id: usize) -> Self::Local {
        todo!()
    }

    fn run_thread(
        &self,
        global: &Self::Global,
        local: &mut Self::Local,
        allocator: &mut B::Allocator,
    ) {
        todo!()
    }
}

struct FlatMap<A: Allocator>([Option<A::Ptr>; 1 << 16]);
