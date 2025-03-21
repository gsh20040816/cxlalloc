use core::fmt::Display;

pub mod boost;
pub mod cxl_shm;
pub mod cxlalloc;
pub mod lightning;
pub mod mimalloc;

pub use boost::Boost;
pub use cxl_shm::CxlShm;
pub use cxlalloc::Cxlalloc;
pub use lightning::Lightning;
pub use mimalloc::Mimalloc;

use serde::Deserialize;
use serde::Serialize;

use clap::ValueEnum;

#[derive(Copy, Clone, Debug, Deserialize, Serialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum Allocator {
    Boost,
    Cxlalloc,
    CxlShm,
    Lightning,
    Mimalloc,
}

impl Display for Allocator {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Allocator::Boost => "boost",
            Allocator::Cxlalloc => "cxlalloc",
            Allocator::CxlShm => "cxl-shm",
            Allocator::Lightning => "lightning",
            Allocator::Mimalloc => "mimalloc",
        };

        write!(f, "{}", name)
    }
}
