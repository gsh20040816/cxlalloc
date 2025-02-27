use clap::Parser;

use crate::Benchmark;
use crate::context;

#[derive(Parser)]
pub struct Cli {
    #[command(flatten)]
    pub context: context::Process,

    #[command(subcommand)]
    pub benchmark: Benchmark,
}
