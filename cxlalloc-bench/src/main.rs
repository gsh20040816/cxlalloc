use std::io::Write as _;
use std::path::PathBuf;

use anyhow::anyhow;
use anyhow::Context;
use clap::Parser;
use cxlalloc_bench::process;
use duct::cmd;

use cxlalloc_bench::Allocator;
use cxlalloc_bench::Benchmark;
use serde::Serialize;

#[derive(Parser, Serialize)]
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
        #[arg(long)]
        pretty: bool,

        #[command(flatten)]
        process: Process,
    },
}

#[derive(Clone, Parser, Serialize)]
#[group(skip)]
struct Process {
    #[arg(short, long)]
    allocator: process::Allocator,

    #[arg(short, long)]
    name: String,

    #[arg(long)]
    node: usize,

    #[arg(short, long)]
    size: usize,

    #[arg(short, long)]
    process_count: usize,

    /// Number of threads per process
    #[arg(short, long)]
    thread_count: usize,

    #[serde(flatten)]
    #[command(subcommand)]
    benchmark: allocator_bench::Benchmark,
}

#[derive(Clone, Parser, Serialize)]
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
        Cli::Process { pretty, process } => {
            (0..process.process_count)
                .map(|process_id| {
                    let mut command = vec![
                        "--allocator".to_string(),
                        process.allocator.to_string(),
                        "--name".to_string(),
                        process.name.to_string(),
                        "--node".to_string(),
                        process.node.to_string(),
                        "--size".to_string(),
                        process.size.to_string(),
                        "--process-count".to_string(),
                        process.process_count.to_string(),
                        "--process-id".to_string(),
                        process_id.to_string(),
                        "--thread-count".to_string(),
                        process.thread_count.to_string(),
                    ];

                    command.extend(process.benchmark.args());

                    duct::cmd(
                        if cfg!(debug_assertions) {
                            "target/debug/cxlalloc-bench-worker"
                        } else {
                            "target/release/cxlalloc-bench-worker"
                        },
                        command,
                    )
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
                .map(|outputs| Metrics {
                    inputs: &process,
                    outputs,
                })
                .for_each(|output| {
                    let mut stdout = std::io::stdout().lock();
                    let write = match pretty {
                        true => serde_json::to_writer_pretty,
                        false => serde_json::to_writer,
                    };
                    write(&mut stdout, &output).unwrap();
                    stdout.write_all(b"\n").unwrap();
                });
        }
    }

    Ok(())
}

#[derive(Serialize)]
pub struct Metrics<'a> {
    #[serde(flatten)]
    inputs: &'a Process,
    #[serde(flatten)]
    outputs: allocator_bench::Metrics,
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
