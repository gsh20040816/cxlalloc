use core::fmt::Display;

use clap::ValueEnum;

mod boost;
pub mod cxl_shm;
mod cxlalloc;

pub use boost::Boost;
pub use cxl_shm::CxlShm;
pub use cxlalloc::Cxlalloc;

#[derive(Clone, ValueEnum)]
pub enum Allocator {
    Boost,
    Cxlalloc,
    CxlShm,
}

impl Display for Allocator {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Allocator::Boost => "boost",
            Allocator::Cxlalloc => "cxlalloc",
            Allocator::CxlShm => "cxl-shm",
        };

        write!(f, "{}", name)
    }
}
