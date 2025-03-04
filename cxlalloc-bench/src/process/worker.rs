use std::io;

use clap::Parser;
use cxlalloc_bench::process::Allocator;

fn main() {
    let cli = cxlalloc_bench::process::Cli::try_parse().unwrap_or_else(|_| {
        let stdin = io::stdin().lock();
        serde_json::from_reader(stdin).unwrap()
    });

    match cli.allocator {
        Allocator::Boost => cli.run::<cxlalloc_bench::process::boost::Backend>(),
        Allocator::Cxlalloc => cli.run::<cxlalloc_bench::process::cxlalloc::Backend>(),
        Allocator::CxlShm => cli.run::<cxlalloc_bench::process::cxl_shm::Backend>(),
        Allocator::Lightning => cli.run::<cxlalloc_bench::process::lightning::Backend>(),
    }
}
