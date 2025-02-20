use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;

use clap::Parser;
use rkyv::ser::writer::IoWriter;

#[derive(Parser)]
struct Cli {
    /// Path to YCSB script or binary
    #[arg(long, default_value = "bin/ycsb")]
    path: PathBuf,

    /// Path to write serialized trace
    #[arg(short, long)]
    output: PathBuf,

    /// Arguments to forward to YCSB
    args: Vec<String>,
}

pub fn main() {
    let cli = Cli::parse();

    let mut output = File::options()
        .create_new(true)
        .write(true)
        .open(&cli.output)
        .map(BufWriter::new)
        .map(IoWriter::new)
        .unwrap_or_else(|error| panic!("Failed to open output file {:?}: {:?}", cli.output, error));

    let stdout = Command::new(&cli.path)
        .args(cli.args)
        .stdout(Stdio::piped())
        .spawn()
        .and_then(|handle| handle.wait_with_output())
        .map(|output| String::from_utf8(output.stdout))
        .unwrap_or_else(|error| {
            panic!(
                "Failed to execute YCSB command {:?}: {:?}",
                &cli.path, error
            )
        })
        .expect("Expected UTF-8 output from YCSB");

    let trace = stdout
        .lines()
        .filter_map(ycsb::Command::parse)
        .collect::<Vec<_>>();

    rkyv::api::high::to_bytes_in::<_, rkyv::rancor::Error>(&trace, &mut output)
        .expect("Failed to serialize trace");
}
