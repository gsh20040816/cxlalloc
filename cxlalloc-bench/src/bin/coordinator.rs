use std::io;

use cxlalloc_bench::Observation;

fn main() -> anyhow::Result<()> {
    let stdin = io::stdin().lock();
    let mut stdout = std::io::stdout().lock();
    let config = serde_json::from_reader::<_, cxlalloc_bench::Config>(stdin)?;
    (0..config.config_global.process_count)
        .map(|process_id| {
            let command = serde_json::to_vec(&config.with_process_id(process_id)).unwrap();
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
                .map(serde_json::from_slice::<allocator_bench::Output>)
                .collect::<Vec<_>>()
        })
        .map(Result::unwrap)
        .map(|output| Observation {
            config: config.clone(),
            output,
        })
        .for_each(|output| {
            serde_json::to_writer(&mut stdout, &output).unwrap();
        });

    Ok(())
}
