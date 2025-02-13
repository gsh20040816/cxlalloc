use allocator_bench::Backend;
use clap::Parser;
use cxlalloc_bench::process::Allocator;

#[derive(Parser)]
#[group(skip)]
struct Cli {
    #[arg(short, long)]
    allocator: Allocator,

    #[arg(short, long)]
    name: String,

    #[arg(short, long)]
    size: usize,

    #[command(flatten)]
    bench: allocator_bench::process::Cli,
}

impl Cli {
    fn run<B: Backend>(&self) {
        match &self.bench.benchmark {
            allocator_bench::Benchmark::ThreadTest(thread_test) => {
                <_ as allocator_bench::benchmark::Interface<B>>::run_process(
                    thread_test,
                    self.bench.process_count,
                    self.bench.process_id,
                    self.bench.thread_count,
                    &self.name,
                    self.size,
                )
            }
            allocator_bench::Benchmark::Ycsb(ycsb) => {
                <_ as allocator_bench::benchmark::Interface<B>>::run_process(
                    ycsb,
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
        Allocator::CxlShm => cli.run::<cxlalloc_bench::process::cxl_shm::Backend>(),
        Allocator::Lightning => cli.run::<cxlalloc_bench::process::lightning::Backend>(),
    }
}
