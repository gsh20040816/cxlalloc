use std::path::PathBuf;

use anyhow::anyhow;
use anyhow::Context;
use clap::Parser;
use clap::ValueEnum;
use duct::cmd;

use cxlalloc_bench::Allocator;
use cxlalloc_bench::Benchmark;

#[derive(Parser)]
enum Cli {
    Run {
        /// Root of mimalloc-bench directory
        #[arg(long, default_value = "extern/mimalloc-bench")]
        root: PathBuf,

        #[arg(short, long)]
        threads: Option<usize>,

        /// Allocator name
        #[arg(short, long)]
        allocator: Allocator,

        /// Benchmark name
        #[arg(short, long)]
        benchmark: Benchmark,

        /// Wrapper program
        #[arg(short, long)]
        wrapper: Option<Wrapper>,
    },

    Bench {
        /// Root of mimalloc-bench directory
        #[arg(long, default_value = "extern/mimalloc-bench")]
        root: PathBuf,

        #[arg(short, long)]
        threads: Option<usize>,

        #[arg(short, long, value_delimiter = ',')]
        allocators: Vec<Allocator>,

        #[arg(short, long, value_delimiter = ',')]
        benchmarks: Vec<Benchmark>,

        #[arg(short, long, default_value_t = 3)]
        warmup: usize,

        #[arg(short, long)]
        output: String,
    },
}

#[derive(Copy, Clone, ValueEnum)]
enum Wrapper {
    Gdb,
    Rr,
    PerfRecord,
    PerfStat,
}

fn main() -> anyhow::Result<()> {
    match Cli::parse() {
        Cli::Bench {
            root,
            threads,
            allocators,
            benchmarks,
            warmup,
            output,
        } => {
            let mut iter = allocators
                .iter()
                .map(|allocator| {
                    let path = root.join(allocator.path());
                    path.canonicalize()
                        .with_context(|| anyhow!("Failed to find path {}", path.display()))
                })
                .collect::<anyhow::Result<Vec<_>>>()?
                .into_iter()
                .map(|allocator| allocator.display().to_string());

            let mut allocators = iter.next().unwrap_or_default();
            for allocator in iter {
                allocators.push(',');
                allocators.push_str(&allocator);
            }

            cmd![
                "hyperfine",
                "--warmup",
                warmup.to_string(),
                "--export-json",
                format!("{}.json", output),
                "--export-markdown",
                format!("{}.md", output),
                "--parameter-list",
                "allocator",
                allocators,
            ]
            .before_spawn(move |command| {
                for benchmark in &benchmarks {
                    command.arg(format!(
                        "env LD_PRELOAD={{allocator}} {}",
                        benchmark.command(threads),
                    ));
                }

                Ok(())
            })
            .run()
            .map(drop)
            .context("Failed to run command")?
        }
        Cli::Run {
            root,
            threads,
            allocator,
            benchmark,
            wrapper,
        } => {
            let path = root.join(allocator.path());
            let ld = format!(
                "LD_PRELOAD={}",
                path.canonicalize()
                    .with_context(|| anyhow!("Failed to find path {}", path.display()))?
                    .display()
            );

            match &wrapper {
                None => cmd!["env", ld],
                Some(Wrapper::Rr) => cmd!["rr", "record", format!("--env={}", ld)],
                Some(Wrapper::Gdb) => cmd!["gdb", "--ex=run", "--args", "env", ld],
                Some(Wrapper::PerfRecord) => {
                    cmd![
                        "perf",
                        "record",
                        "--call-graph",
                        "dwarf",
                        // "-e",
                        // "branch-misses:pp",
                        "-o",
                        "perf.data",
                        "env",
                        ld,
                    ]
                }
                Some(Wrapper::PerfStat) => {
                    cmd!["perf", "stat", "env", ld]
                }
            }
            .before_spawn(move |command| {
                command.arg(root.join(benchmark.path()));
                command.args(benchmark.args(threads));
                Ok(())
            })
            .run()
            .context("Failed to run command")?;

            if let Some(Wrapper::PerfRecord) = wrapper {
                let home = homedir::my_home()?.expect("No home directory");
                cmd!["perf", "script", "--input", "perf.data"]
                    .pipe(cmd!(home.join(".cargo/bin/inferno-collapse-perf")))
                    .pipe(cmd!(home.join(".cargo/bin/inferno-flamegraph")))
                    .stdout_path("out.svg")
                    .run()?;
            }
        }
    }

    Ok(())
}
