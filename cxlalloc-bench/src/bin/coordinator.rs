use shm::Barrier;
use std::io;
use std::io::Write as _;

fn main() -> anyhow::Result<()> {
    let stdin = io::stdin().lock();
    let mut stdout = std::io::stdout().lock();
    let config = serde_json::from_reader::<_, cxlalloc_bench::Config>(stdin)?;

    if let Some(numa) = &config.global.numa {
        numa.set_mempolicy()?;
    }

    // Initialize barrier for processes to synchronize on
    Barrier::builder()
        .name("barrier-process".to_owned())
        .create(true)
        .thread_count(u32::try_from(config.global.process_count).unwrap())
        .build()?;

    Barrier::builder()
        .name("barrier-thread".to_owned())
        .create(true)
        .thread_count(
            // Account for one coordinator thread per process
            u32::try_from(config.global.thread_count + config.global.process_count).unwrap(),
        )
        .build()?;

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
