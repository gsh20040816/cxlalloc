use core::sync::atomic::AtomicU64;
use core::sync::atomic::Ordering;
use std::path::PathBuf;

use anyhow::anyhow;
use anyhow::Context;
use clap::Parser;
use cxlalloc_bench::ProcessAllocator;
use duct::cmd;

use cxlalloc_bench::Allocator;
use cxlalloc_bench::Benchmark;
use process_bench::Allocator as _;

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
        #[command(subcommand)]
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

    Process {
        #[arg(short, long)]
        allocator: ProcessAllocator,

        #[arg(short, long)]
        name: String,

        #[arg(short, long)]
        size: usize,

        #[arg(short, long)]
        processes: u64,
    },
}

#[derive(Clone, Parser)]
enum Wrapper {
    Gdb,
    Rr,

    PerfRecord {
        /// CPU list for taskset
        #[arg(long)]
        pin: String,

        #[arg(long, value_delimiter = ',')]
        bases: Vec<String>,
    },
    PerfStat {
        /// CPU list for taskset
        #[arg(long)]
        pin: String,
    },
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
            let path_allocator = root.join(allocator.path());
            let path_benchmark = root.join(benchmark.path());

            let ld_preload = format!(
                "LD_PRELOAD={}",
                path_allocator
                    .canonicalize()
                    .with_context(|| anyhow!("Failed to find path {}", path_allocator.display()))?
                    .display()
            );

            let ld = ld_preload.clone();
            wrapper
                .as_ref()
                .map(Wrapper::prefix)
                .map(|wrapper| {
                    wrapper.before_spawn(move |command| {
                        command.arg("env");
                        command.arg(&ld);
                        command.arg(&path_benchmark);
                        command.args(benchmark.args(threads));
                        Ok(())
                    })
                })
                .unwrap_or_else(|| cmd!("env", ld_preload))
                .run()
                .context("Failed to run command")?;

            if let Some(Wrapper::PerfRecord { bases, .. }) = wrapper {
                let home = homedir::my_home()?.expect("No home directory");

                let flamegraph = duct::cmd(
                    home.join(".cargo/bin/inferno-flamegraph"),
                    bases.iter().flat_map(|base| ["--base", base]),
                );

                cmd!["perf", "script", "--input", "perf.data"]
                    .pipe(cmd!(home.join(".cargo/bin/inferno-collapse-perf")))
                    .pipe(flamegraph)
                    .stdout_path("out.svg")
                    .run()?;
            }
        }
        Cli::Process {
            allocator,
            name,
            size,
            processes,
        } => {
            let barrier = match allocator {
                ProcessAllocator::Boost => {
                    let mut shm = cxlalloc_bench::boost::Boost::open(&name, size);
                    let pointer = shm.allocate(64);
                    unsafe {
                        AtomicU64::from_ptr(pointer.cast()).store(processes, Ordering::Relaxed);
                        shm.address_to_offset(pointer)
                    }
                }
            };

            (0..processes)
                .map(|process| {
                    cmd![
                        "target/release/cxlalloc-bench-worker",
                        "--allocator",
                        allocator.to_string(),
                        "--name",
                        &name,
                        "--size",
                        size.to_string(),
                        "--process-id",
                        process.to_string(),
                        "--barrier",
                        barrier.to_string(),
                    ]
                    .start()
                    .unwrap()
                })
                .collect::<Vec<_>>()
                .into_iter()
                .for_each(|handle| {
                    handle.wait().unwrap();
                })
        }
    }

    Ok(())
}

impl Wrapper {
    fn prefix(&self) -> duct::Expression {
        match self {
            Wrapper::Rr => cmd!["rr", "record"],
            Wrapper::Gdb => cmd!["gdb", "--ex=run", "--args"],
            Wrapper::PerfRecord { pin, bases: _ } => {
                cmd![
                    "taskset",
                    "-c",
                    pin,
                    "perf",
                    "record",
                    "--call-graph",
                    // https://gist.github.com/dlaehnemann/df31787c41bd50c0fe223df07cf6eb89
                    "dwarf,16384",
                    "-F",
                    "9997",
                    "--strict-freq",
                    "-o",
                    "perf.data",
                ]
            }
            Wrapper::PerfStat { pin } => {
                cmd!["taskset", "-c", pin, "perf", "stat"]
            }
        }
    }
}
