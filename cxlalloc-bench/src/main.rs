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
    #[arg(short, long, value_delimiter = ',', default_value = "cxlalloc,cxl-shm")]
    allocator: Vec<Allocator>,

    #[arg(short, long, value_delimiter = ',', default_value = "1")]
    process_count: Vec<usize>,

    #[arg(long, default_value_t = 1)]
    allocator_numa: usize,

    #[arg(long, default_value_t = 2usize.pow(35))]
    allocator_size: usize,

    #[arg(long, value_delimiter = ',', default_value = "false,true")]
    allocator_populate: Vec<bool>,

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
        #[arg(long, default_value_t = 10_000_000)]
        record_count: usize,

        #[arg(long, default_value_t = 1_000_000)]
        operation_count: usize,

        #[arg(short, long, value_delimiter = ',', default_value = "linear,linked")]
        index: Vec<Index>,

        #[arg(long, default_value_t = 1 << 25)]
        index_len: usize,

        #[arg(long, value_delimiter = ',', default_value = "false,true")]
        index_inline: Vec<bool>,

        #[arg(long, value_delimiter = ',', default_value = "false,true")]
        index_populate: Vec<bool>,

        /// Whether to write value or not
        #[arg(long, value_delimiter = ',', default_value = "false,true")]
        write: Vec<bool>,

        #[command(subcommand)]
        workload: Workload,
    },
    Xmalloc {
        #[arg(short, long, value_delimiter = ',', default_value = "1,2,4,8,16,32,40")]
        thread_total: Vec<usize>,
    },
}

#[derive(Parser)]
enum Workload {
    Load {
        #[arg(short, long, value_delimiter = ',', default_value = "1,2,4,8,16,32,40")]
        thread_total: Vec<usize>,
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
        insert_proportion: Vec<f32>,
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
            record_count,
            operation_count,
            index,
            index_len,
            index_inline,
            index_populate,
            write,
            workload: Workload::Load { thread_total },
        } => {
            for (
                &allocator,
                &allocator_populate,
                &index,
                &index_inline,
                &index_populate,
                &write,
                &process_count,
                &thread_total,
            ) in cartesian!(
                cli.allocator.iter(),
                cli.allocator_populate.iter(),
                index.iter(),
                index_inline.iter(),
                index_populate.iter(),
                write.iter(),
                cli.process_count.iter(),
                thread_total.iter()
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
                    &cxlalloc_bench::Config {
                        allocator,
                        index,
                        config_allocator: allocator_bench::allocator::Config::builder()
                            .numa(cli.allocator_numa)
                            .size(cli.allocator_size)
                            .populate(allocator_populate)
                            .build(),
                        config_global: allocator_bench::config::Global::builder()
                            .process_count(process_count)
                            .thread_count(thread_count)
                            .build(),
                        config_benchmark: allocator_bench::benchmark::Config::Ycsb(
                            allocator_bench::benchmark::ycsb::Ycsb::builder()
                                .load(true)
                                .write(write)
                                .index(
                                    allocator_bench::index::Config::builder()
                                        .inline(index_inline)
                                        .populate(index_populate)
                                        .len(*index_len)
                                        .build(),
                                )
                                .workload(
                                    ycsb::Workload::builder()
                                        .record_count(*record_count)
                                        .operation_count(*operation_count)
                                        .build(),
                                )
                                .build(),
                        ),
                    },
                    &mut out,
                )?;
            }
        }
        Experiment::Ycsb {
            index,
            record_count,
            operation_count,
            index_len,
            index_inline,
            index_populate,
            write,
            workload:
                Workload::D {
                    thread_total,
                    insert_proportion,
                },
        } => {
            for (
                &allocator,
                &allocator_populate,
                &index,
                &index_inline,
                &index_populate,
                &write,
                &process_count,
                &insert_proportion,
            ) in cartesian!(
                cli.allocator.iter(),
                cli.allocator_populate.iter(),
                index.iter(),
                index_inline.iter(),
                index_populate.iter(),
                write.iter(),
                cli.process_count.iter(),
                insert_proportion.iter()
            ) {
                if thread_total % process_count != 0 {
                    continue;
                }

                eprintln!("{:16} | {:8?} | {:4}", allocator, index, insert_proportion);

                let thread_count = thread_total / process_count;

                cli.run(
                    &cxlalloc_bench::Config {
                        allocator,
                        index,
                        config_allocator: allocator_bench::allocator::Config::builder()
                            .numa(cli.allocator_numa)
                            .size(cli.allocator_size)
                            .populate(allocator_populate)
                            .build(),
                        config_global: allocator_bench::config::Global::builder()
                            .process_count(process_count)
                            .thread_count(thread_count)
                            .build(),
                        config_benchmark: allocator_bench::benchmark::Config::Ycsb(
                            allocator_bench::benchmark::ycsb::Ycsb::builder()
                                .load(false)
                                .write(write)
                                .index(
                                    allocator_bench::index::Config::builder()
                                        .len(*index_len)
                                        .inline(index_inline)
                                        .populate(index_populate)
                                        .build(),
                                )
                                .workload(ycsb::Workload {
                                    record_count: *record_count,
                                    operation_count: *operation_count,
                                    insert_proportion,
                                    read_proportion: 1.0 - insert_proportion,
                                    ..ycsb::workload::D.clone()
                                })
                                .build(),
                        ),
                    },
                    &mut out,
                )?;
            }
        }
        Experiment::Xmalloc { thread_total } => {
            for (&allocator, &allocator_populate, &process_count, &thread_total) in cartesian!(
                cli.allocator.iter(),
                cli.allocator_populate.iter(),
                cli.process_count.iter(),
                thread_total.iter()
            ) {
                if thread_total % process_count != 0 {
                    continue;
                }

                eprintln!("xmalloc | {:16}", allocator);

                let thread_count = thread_total / process_count;

                cli.run(
                    &cxlalloc_bench::Config::builder()
                        .allocator(allocator)
                        .index(Index::Linear)
                        .config_global(
                            allocator_bench::config::Global::builder()
                                .process_count(process_count)
                                .thread_count(thread_count)
                                .build(),
                        )
                        .config_allocator(
                            allocator_bench::allocator::Config::builder()
                                .numa(0)
                                .populate(allocator_populate)
                                .size(1 << 32)
                                .build(),
                        )
                        .config_benchmark(allocator_bench::benchmark::Config::Xmalloc(
                            allocator_bench::benchmark::Xmalloc::builder().build(),
                        ))
                        .build(),
                    &mut out,
                )?;
            }
        }
    }

    Ok(())
}

impl Cli {
    fn run(&self, cli: &cxlalloc_bench::Config, out: &mut File) -> anyhow::Result<()> {
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
