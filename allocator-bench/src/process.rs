use clap::Parser;
use serde::Serialize;

use crate::Benchmark;
use crate::context;

#[derive(Clone, Parser, Serialize)]
pub struct Cli {
    #[command(flatten)]
    pub context: context::Process,

    #[command(subcommand)]
    pub benchmark: Benchmark,
}
