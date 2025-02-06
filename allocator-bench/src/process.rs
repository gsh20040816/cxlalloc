use clap::Parser;

use crate::Benchmark;

#[derive(Parser)]
pub struct Cli {
    /// Number of processes
    #[arg(long)]
    pub process_count: usize,

    /// Unique process ID within range 0..process_count
    #[arg(long)]
    pub process_id: usize,

    /// Number of threads per process
    #[arg(long)]
    pub thread_count: usize,

    #[command(subcommand)]
    pub benchmark: Benchmark,
}
