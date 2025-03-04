use std::io;

use cxlalloc_bench::process::Allocator;

fn main() {
    let stdin = io::stdin().lock();
    let cli = serde_json::from_reader::<_, cxlalloc_bench::process::Cli>(stdin).unwrap();

    match cli.allocator {
        Allocator::Boost => cli.run::<cxlalloc_bench::process::boost::Backend>(),
        Allocator::Cxlalloc => cli.run::<cxlalloc_bench::process::cxlalloc::Backend>(),
        Allocator::CxlShm => cli.run::<cxlalloc_bench::process::cxl_shm::Backend>(),
        Allocator::Lightning => cli.run::<cxlalloc_bench::process::lightning::Backend>(),
    }
}
