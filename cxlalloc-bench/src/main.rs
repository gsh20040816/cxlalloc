use clap::Parser;
use clap::ValueEnum;
use duct::cmd;

use cxlalloc_bench::Allocator;
use cxlalloc_bench::Benchmark;

#[derive(Parser)]
enum Cli {
    Run {
        #[arg(short, long)]
        allocator: Allocator,

        #[arg(short, long)]
        benchmark: Benchmark,

        #[arg(short, long)]
        release: bool,

        #[arg(short, long)]
        wrapper: Option<Wrapper>,
    },

    Bench {
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

fn main() {
    match Cli::parse() {
        Cli::Bench {
            allocators,
            benchmarks,
            warmup,
            output,
        } => cmd![
            "hyperfine",
            "--warmup",
            warmup.to_string(),
            "--export-json",
            format!("{}.json", output),
            "--export-markdown",
            format!("{}.md", output),
        ]
        .before_spawn(move |command| {
            command.arg("--parameter-list");
            command.arg("allocator");

            let mut iter = allocators
                .iter()
                .map(|allocator| allocator.path(true).display().to_string());
            let mut list = iter.next().unwrap_or_default();
            for allocator in iter {
                list.push(',');
                list.push_str(&allocator);
            }
            command.arg(list);

            for benchmark in &benchmarks {
                command.arg(format!(
                    "env LD_PRELOAD={{allocator}} {}",
                    benchmark.command(),
                ));
            }

            Ok(())
        })
        .run()
        .map(drop)
        .unwrap(),
        Cli::Run {
            allocator,
            benchmark,
            release,
            wrapper,
        } => {
            let ld = format!("LD_PRELOAD={}", allocator.path(release).display());

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
                command.arg(benchmark.path());
                command.args(benchmark.args());
                Ok(())
            })
            .run()
            .unwrap();

            if let Some(Wrapper::PerfRecord) = wrapper {
                let home = homedir::my_home().unwrap().unwrap();
                cmd!["perf", "script", "--input", "perf.data"]
                    .pipe(cmd!(home.join(".cargo/bin/inferno-collapse-perf")))
                    .pipe(cmd!(home.join(".cargo/bin/inferno-flamegraph")))
                    .stdout_path("out.svg")
                    .run()
                    .unwrap();
            }
        }
    }
}
