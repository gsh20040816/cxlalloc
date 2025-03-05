use std::io;

use cxlalloc_bench::Allocator;

fn main() {
    let stdin = io::stdin().lock();
    let cli = serde_json::from_reader::<_, cxlalloc_bench::allocator::Cli>(stdin).unwrap();

    match cli.allocator {
        Allocator::Boost => cli.run::<cxlalloc_bench::allocator::boost::Backend>(),
        Allocator::Cxlalloc => cli.run::<cxlalloc_bench::allocator::cxlalloc::Backend>(),
        Allocator::CxlShm => cli.run::<cxlalloc_bench::allocator::cxl_shm::Backend>(),
        Allocator::Lightning => cli.run::<cxlalloc_bench::allocator::lightning::Backend>(),
    }
}
