mod allocator;
mod benchmark;
pub mod process;

pub use allocator::Allocator;
pub use benchmark::Benchmark;

use clap::Parser;
use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Parser, Deserialize, Serialize)]
#[group(skip)]
pub struct Cli {
    #[arg(short, long)]
    pub allocator: process::Allocator,

    #[serde(skip_serializing)]
    #[arg(long)]
    pub pretty: bool,

    #[arg(short, long)]
    pub size: usize,

    #[serde(flatten)]
    #[command(flatten)]
    pub context: allocator_bench::context::Global,

    #[serde(flatten)]
    #[command(subcommand)]
    pub benchmark: allocator_bench::Benchmark,
}

#[derive(Serialize)]
pub struct Observation {
    #[serde(flatten)]
    pub inputs: Cli,

    #[serde(flatten)]
    pub outputs: allocator_bench::Metrics,
}
