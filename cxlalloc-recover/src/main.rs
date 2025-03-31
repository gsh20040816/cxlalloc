use std::fs;
use std::fs::File;
use std::path::PathBuf;

use cartesian::cartesian;
use cartesian::TuplePrepend as _;
use clap::Parser;
use cxlalloc_recover::worker::Workload;

#[derive(Parser)]
pub struct Cli {
    #[arg(long, value_delimiter = ',', default_value = "queue,clevel")]
    workload: Vec<Workload>,

    #[arg(long, value_delimiter = ',', default_value = "1,2,4,8")]
    crash_count: Vec<u64>,

    #[arg(long, value_delimiter = ',', default_value = "false,true")]
    block: Vec<bool>,

    #[arg(long, default_value = if cfg!(debug_assertions) {
        "target/debug/cxlalloc-recover-worker"
    } else {
        "target/release/cxlalloc-recover-worker"
    })]
    worker: PathBuf,

    #[arg(long, default_value = "recover.ndjson")]
    output: PathBuf,
}

fn main() {
    let cli = Cli::parse();
    let output = File::options()
        .create(true)
        .append(true)
        .open(&cli.output)
        .unwrap();

    const PATH: &str = "/dev/shm/pool";

    cartesian!(
        cli.workload.iter(),
        cli.crash_count.iter(),
        cli.block.iter()
    )
    .map(|(workload, crash_count, block)| {
        cxlalloc_recover::worker::Config::builder()
            .allocator(cxlalloc_recover::worker::Allocator::default())
            .crash_victim(2)
            .crash_count(*crash_count)
            .block(*block)
            .path(PATH.to_owned())
            .object_count(1_000_000)
            .thread_count(40)
            .heap_size(1 << 36)
            .workload(workload.clone())
            .build()
    })
    .map(|config| serde_json::to_vec(&config).unwrap())
    .inspect(|_| fs::remove_file(PATH).unwrap())
    .try_for_each(|config| {
        let empty: [String; 0] = [];
        duct::cmd(&cli.worker, empty)
            .stdin_bytes(config)
            .stdout_file(output.try_clone().unwrap())
            .start()
            .unwrap()
            .wait()
            .map(drop)
    })
    .unwrap();
}
