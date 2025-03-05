use std::io;
use std::io::Write as _;

use cxlalloc_bench::allocator;
use cxlalloc_bench::Observation;

fn main() -> anyhow::Result<()> {
    let stdin = io::stdin().lock();
    let cli = serde_json::from_reader::<_, cxlalloc_bench::Cli>(stdin)?;
    (0..cli.control.process_count)
        .map(|process_id| {
            let command = serde_json::to_vec(&allocator::Cli {
                allocator: cli.allocator.clone(),
                context: allocator_bench::context::Process {
                    global: cli.control,
                    process_id,
                },
                benchmark: cli.benchmark.clone(),
            })
            .unwrap();
            let empty: [String; 0] = [];

            duct::cmd(
                if cfg!(debug_assertions) {
                    "target/debug/cxlalloc-bench-worker"
                } else {
                    "target/release/cxlalloc-bench-worker"
                },
                empty,
            )
            .stdin_bytes(command)
            .stdout_capture()
            .start()
            .unwrap()
        })
        .collect::<Vec<_>>()
        .into_iter()
        .map(|handle| handle.into_output().unwrap().stdout)
        .flat_map(|stdout| {
            stdout
                .trim_ascii()
                .split(|byte| *byte == b'\n')
                .map(serde_json::from_slice::<allocator_bench::Metrics>)
                .collect::<Vec<_>>()
        })
        .map(Result::unwrap)
        .map(|outputs| Observation {
            inputs: cli.clone(),
            outputs,
        })
        .for_each(|output| {
            let mut stdout = std::io::stdout().lock();
            serde_json::to_writer(&mut stdout, &output).unwrap();
            stdout.write_all(b"\n").unwrap();
        });

    Ok(())
}
