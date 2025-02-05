use clap::Parser;
use cxlalloc_bench::process::Allocator;

#[derive(Parser)]
struct Cli {
    #[arg(short, long)]
    allocator: Allocator,

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
        Allocator::Boost => {
            process_bench::worker::run::<cxlalloc_bench::process::Boost>(
                &cli.name,
                cli.size,
                cli.process_id,
                cli.barrier,
            );
        }
    }
}
