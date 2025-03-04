use clap::Parser;
use serde::Deserialize;
use serde::Serialize;

use crate::Benchmark;
use crate::context;

#[derive(Clone, Parser, Deserialize, Serialize)]
pub struct Cli {
    #[command(flatten)]
    pub context: context::Process,

    #[command(subcommand)]
    pub benchmark: Benchmark,
}
