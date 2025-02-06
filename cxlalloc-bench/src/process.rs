use core::fmt::Display;

use clap::ValueEnum;

mod boost;
mod cxlalloc;
pub mod cxlmalloc;

pub use boost::Boost;
pub use cxlalloc::Cxlalloc;
pub use cxlmalloc::Cxlmalloc;

#[derive(Clone, ValueEnum)]
pub enum Allocator {
    Boost,
    Cxlalloc,
    Cxlmalloc,
}

impl Display for Allocator {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Allocator::Boost => "boost",
            Allocator::Cxlalloc => "cxlalloc",
            Allocator::Cxlmalloc => "cxlmalloc",
        };

        write!(f, "{}", name)
    }
}
