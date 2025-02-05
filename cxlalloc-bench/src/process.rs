use core::fmt::Display;

use clap::ValueEnum;

mod boost;
mod cxlmalloc;

pub use boost::Boost;

#[derive(Clone, ValueEnum)]
pub enum Allocator {
    Boost,
}

impl Display for Allocator {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Allocator::Boost => "boost",
        };

        write!(f, "{}", name)
    }
}
