use clap::Parser;
use cxlalloc_bench::ProcessAllocator;

#[derive(Parser)]
struct Cli {
    #[arg(short, long)]
    allocator: ProcessAllocator,

    #[arg(short, long)]
    name: String,

    #[arg(short, long)]
    size: usize,

    #[arg(short, long)]
    process_id: u64,

    #[arg(short, long)]
    barrier: u64,
}

fn main() {
    let cli = Cli::parse();

    match cli.allocator {
        ProcessAllocator::Boost => {
            process_bench::worker::run::<cxlalloc_bench::boost::Boost>(
                &cli.name,
                cli.size,
                cli.process_id,
                cli.barrier,
            );
        }
    }
}
