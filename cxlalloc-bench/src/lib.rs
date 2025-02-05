mod allocator;
mod benchmark;
pub mod boost;
pub mod cxlmalloc;

use core::fmt::Display;

pub use allocator::Allocator;
pub use benchmark::Benchmark;

use clap::ValueEnum;

#[derive(Clone, ValueEnum)]
pub enum ProcessAllocator {
    Boost,
}

impl Display for ProcessAllocator {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            ProcessAllocator::Boost => "boost",
        };

        write!(f, "{}", name)
    }
}
