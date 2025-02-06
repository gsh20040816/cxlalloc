use clap::Parser;
use cxlalloc_bench::process::Allocator;
use process_bench::Backend;

#[derive(Parser)]
struct Cli {
    #[arg(short, long)]
    allocator: Allocator,

    #[arg(short, long)]
    name: String,

    #[arg(short, long)]
    size: usize,

    #[command(flatten)]
    bench: process_bench::process::Cli,
}

impl Cli {
    fn run<B: Backend>(&self) {
        match &self.bench.benchmark {
            process_bench::Benchmark::ThreadTest(thread_test) => {
                <_ as process_bench::benchmark::Interface<B>>::run_process(
                    thread_test,
                    self.bench.process_count,
                    self.bench.process_id,
                    self.bench.thread_count,
                    &self.name,
                    self.size,
                )
            }
        }
    }
}

fn main() {
    let cli = Cli::parse();

    match cli.allocator {
        Allocator::Boost => cli.run::<cxlalloc_bench::process::Boost>(),
        Allocator::Cxlalloc => cli.run::<cxlalloc_bench::process::Cxlalloc>(),
        Allocator::Cxlmalloc => cli.run::<cxlalloc_bench::process::cxlmalloc::Backend>(),
    }
}
