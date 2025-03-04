use std::fs::File;

use cxlalloc_bench::process::Allocator;

fn main() -> anyhow::Result<()> {
    let out = File::options()
        .create(true)
        .append(true)
        .open("result.ndjson")
        .unwrap();

    for allocator in [Allocator::Cxlalloc, Allocator::CxlShm] {
        for process_count in [1] {
            for thread_total in [1, 2, 4, 8, 16, 32, 40] {
                let thread_count = thread_total / process_count;

                eprintln!("a{:?} p{:?} t{:?}", allocator, process_count, thread_total);

                let cli = cxlalloc_bench::Cli {
                    allocator: allocator.clone(),
                    size: 2usize.pow(33),
                    context: allocator_bench::context::Global {
                        numa: 0,
                        populate: false,
                        process_count,
                        thread_count,
                    },
                    benchmark: allocator_bench::Benchmark::Ycsb(
                        allocator_bench::benchmark::ycsb::Ycsb {
                            load: true,
                            workload: ycsb::Workload {
                                // record_count: 10_000_000,
                                ..ycsb::workload::A.clone()
                            },
                        },
                    ),
                };
                let cli = serde_json::to_vec(&cli)?;

                let empty: [String; 0] = [];

                duct::cmd(
                    if cfg!(debug_assertions) {
                        "target/debug/cxlalloc-bench-coordinator"
                    } else {
                        "target/release/cxlalloc-bench-coordinator"
                    },
                    empty,
                )
                .stdin_bytes(cli)
                .stdout_file(out.try_clone()?)
                .start()?
                .wait()?;
            }
        }
    }

    Ok(())
}
