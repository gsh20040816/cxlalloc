use std::fs::File;
use std::path::PathBuf;

use anyhow::anyhow;
use cartesian::cartesian;
use cartesian::TuplePrepend as _;
use clap::Parser;
use cxlalloc_bench::Allocator;
use cxlalloc_bench::Index;

#[derive(Parser)]
struct Cli {
    #[arg(short, long, value_delimiter = ',')]
    allocators: Vec<Allocator>,

    #[arg(short, long, value_delimiter = ',', default_value = "1")]
    process_counts: Vec<usize>,

    #[arg(short, long, default_value_t = 1)]
    numa: usize,

    #[arg(short, long, default_value_t = 2usize.pow(34))]
    size: usize,

    #[arg(long)]
    populate: bool,

    #[arg(
        short,
        long,
        default_value = "target/release/cxlalloc-bench-coordinator"
    )]
    coordinator: PathBuf,

    #[arg(short, long, default_value = "result.ndjson")]
    output: PathBuf,

    #[command(subcommand)]
    experiment: Experiment,
}

#[derive(Parser)]
enum Experiment {
    Ycsb {
        #[arg(short, long, value_delimiter = ',')]
        indexes: Vec<Index>,

        #[command(subcommand)]
        workload: Workload,
    },
}

#[derive(Parser)]
enum Workload {
    Load {
        #[arg(short, long, value_delimiter = ',', default_value = "1,2,4,8,16,32,40")]
        thread_totals: Vec<usize>,
    },
    D {
        #[arg(short, long, default_value_t = 40)]
        thread_total: usize,

        #[arg(
            short,
            long,
            value_delimiter = ',',
            default_value = "0.0,0.05,0.10,0.15,0.20,0.25"
        )]
        insert_proportions: Vec<f32>,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let mut out = File::options()
        .create(true)
        .append(true)
        .open(&cli.output)?;

    match &cli.experiment {
        Experiment::Ycsb {
            indexes,
            workload: Workload::Load { thread_totals },
        } => {
            for (&allocator, &index, &process_count, &thread_total) in cartesian!(
                cli.allocators.iter(),
                indexes.iter(),
                cli.process_counts.iter(),
                thread_totals.iter()
            ) {
                if thread_total % process_count != 0 {
                    continue;
                }

                eprintln!(
                    "{:16} | {:3} | {:3}",
                    allocator, process_count, thread_total,
                );

                let thread_count = thread_total / process_count;

                cli.run(
                    &cxlalloc_bench::Cli {
                        allocator,
                        index,
                        control: allocator_bench::context::Global {
                            numa: cli.numa,
                            size: cli.size,
                            populate: cli.populate,
                            process_count,
                            thread_count,
                        },
                        benchmark: allocator_bench::Benchmark::Ycsb(
                            allocator_bench::benchmark::ycsb::Ycsb {
                                load: true,
                                workload: ycsb::Workload {
                                    record_count: 10_000_000,
                                    ..ycsb::workload::A.clone()
                                },
                            },
                        ),
                    },
                    &mut out,
                )?;
            }
        }
        Experiment::Ycsb {
            indexes,
            workload:
                Workload::D {
                    thread_total,
                    insert_proportions,
                },
        } => {
            for (&allocator, &index, &process_count, &insert_proportion) in cartesian!(
                cli.allocators.iter(),
                indexes.iter(),
                cli.process_counts.iter(),
                insert_proportions.iter()
            ) {
                if thread_total % process_count != 0 {
                    continue;
                }

                eprintln!(
                    "{:16} | {:3} | {:3}",
                    allocator, process_count, thread_total,
                );

                let thread_count = thread_total / process_count;

                cli.run(
                    &cxlalloc_bench::Cli {
                        allocator,
                        index,
                        control: allocator_bench::context::Global {
                            numa: cli.numa,
                            size: cli.size,
                            populate: cli.populate,
                            process_count,
                            thread_count,
                        },
                        benchmark: allocator_bench::Benchmark::Ycsb(
                            allocator_bench::benchmark::ycsb::Ycsb {
                                load: false,
                                workload: ycsb::Workload {
                                    record_count: 10_000_000,
                                    insert_proportion,
                                    read_proportion: 1.0 - insert_proportion,
                                    ..ycsb::workload::D.clone()
                                },
                            },
                        ),
                    },
                    &mut out,
                )?;
            }
        }
    }

    Ok(())
}

impl Cli {
    fn run(&self, cli: &cxlalloc_bench::Cli, out: &mut File) -> anyhow::Result<()> {
        const EMPTY: [String; 0] = [];

        let handle = duct::cmd(&self.coordinator, EMPTY)
            .stdin_bytes(serde_json::to_vec(&cli)?)
            .stdout_file(out.try_clone()?)
            .start()?;
        let output = handle.wait()?;

        if !output.status.success() {
            return Err(anyhow!(
                "Command {:?} failed with status code {:?}",
                cli,
                output.status,
            ));
        }

        Ok(())
    }
}
