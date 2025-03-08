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

    #[arg(long, default_value_t = 1)]
    allocator_numa: usize,

    #[arg(long, default_value_t = 2usize.pow(34))]
    allocator_size: usize,

    #[arg(long)]
    allocator_populate: bool,

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

        #[arg(long, default_value_t = 10_000_000)]
        record_count: usize,

        #[arg(long, default_value_t = 1_000_000)]
        operation_count: usize,

        #[arg(long, default_value_t = 1 << 24)]
        index_len: usize,

        #[arg(long)]
        index_inline: bool,

        #[arg(long)]
        index_populate: bool,

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
            default_value = "0.0,0.1,0.2,0.3,0.4,0.5,0.6,0.7,0.8,0.9,1.0"
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
            record_count,
            operation_count,
            index_len,
            index_inline,
            index_populate,
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
                            allocator_numa: cli.allocator_numa,
                            allocator_size: cli.allocator_size,
                            allocator_populate: cli.allocator_populate,
                            process_count,
                            thread_count,
                        },
                        benchmark: allocator_bench::Benchmark::Ycsb(
                            allocator_bench::benchmark::ycsb::Ycsb {
                                load: true,
                                index_len: *index_len,
                                index_inline: *index_inline,
                                index_populate: *index_populate,
                                workload: ycsb::Workload {
                                    record_count: *record_count,
                                    operation_count: *operation_count,
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
            record_count,
            operation_count,
            index_len,
            index_inline,
            index_populate,
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

                eprintln!("{:16} | {:8?} | {:4}", allocator, index, insert_proportion);

                let thread_count = thread_total / process_count;

                cli.run(
                    &cxlalloc_bench::Cli {
                        allocator,
                        index,
                        control: allocator_bench::context::Global {
                            allocator_numa: cli.allocator_numa,
                            allocator_size: cli.allocator_size,
                            allocator_populate: cli.allocator_populate,
                            process_count,
                            thread_count,
                        },
                        benchmark: allocator_bench::Benchmark::Ycsb(
                            allocator_bench::benchmark::ycsb::Ycsb {
                                load: false,
                                index_len: *index_len,
                                index_inline: *index_inline,
                                index_populate: *index_populate,
                                workload: ycsb::Workload {
                                    record_count: *record_count,
                                    operation_count: *operation_count,
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
