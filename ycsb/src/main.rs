use std::path::PathBuf;

use clap::Parser;
use ycsb::Workload;

#[derive(Parser)]
struct Cli {
    #[arg(short, long)]
    path: PathBuf,
}

pub fn main() {
    let cli = Cli::parse();

    let workload =
        toml::from_str::<Workload>(&std::fs::read_to_string(&cli.path).unwrap()).unwrap();

    dbg!(&workload);
}
