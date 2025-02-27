mod allocator;
mod benchmark;
pub mod process;

use std::env;
use std::sync::LazyLock;

pub use allocator::Allocator;
pub use benchmark::Benchmark;

static MAP_POPULATE: LazyLock<bool> = LazyLock::new(|| {
    env::var("CXLALLOC_MAP_POPULATE")
        .is_ok_and(|bool| bool.parse::<bool>().is_ok_and(std::convert::identity))
});
