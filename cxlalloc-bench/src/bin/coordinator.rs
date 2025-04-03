use std::io;
use std::io::Write as _;

use allocator_bench::Barrier;

fn main() -> anyhow::Result<()> {
    let stdin = io::stdin().lock();
    let mut stdout = std::io::stdout().lock();
    let config = serde_json::from_reader::<_, cxlalloc_bench::Config>(stdin)?;

    // Initialize barrier for processes to synchronize on
    Barrier::new(true, 0)?;

    (0..config.global.process_count)
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
        .try_for_each(|output| -> anyhow::Result<()> {
            stdout.write_all(&output)?;
            stdout.write_all(b"\n")?;
            Ok(())
        })
}
