use std::fs::File;
use std::path::PathBuf;

use allocator_bench::allocator::Consistency;
use anyhow::anyhow;
use cartesian::cartesian;
use clap::Parser;
use cxlalloc_bench::Allocator;
use cxlalloc_bench::Index;

const CONSISTENCY: Consistency = if cfg!(feature = "consistency-sfence") {
    Consistency::Sfence
} else if cfg!(feature = "consistency-clflush") {
    Consistency::Clflush
} else if cfg!(feature = "consistency-clflushopt") {
    Consistency::Clflushopt
} else {
    Consistency::None
};

#[derive(Parser)]
struct Config {
    #[arg(short, long, value_delimiter = ',', default_value = "1")]
    process_count: Vec<usize>,

    #[arg(short, long, value_delimiter = ',', default_value = "1,2,4,8,16,32,40")]
    thread_count: Vec<usize>,

    #[arg(
        short,
        long,
        value_delimiter = ',',
        default_value = "cxlalloc,cxl-shm,boost,lightning,mimalloc,ralloc"
    )]
    allocator: Vec<Allocator>,

    #[arg(long, value_delimiter = ';', default_value = "null")]
    allocator_config: Vec<String>,

    #[arg(long, value_delimiter = ',', default_value = "0")]
    allocator_numa: Vec<usize>,

    // 2^36 = 64 GiB
    #[arg(long, value_delimiter = ',', default_value = "68719476736")]
    allocator_size: Vec<usize>,

    #[arg(long, value_delimiter = ',', default_value = "false")]
    allocator_populate: Vec<bool>,

    #[arg(
        short,
        long,
        default_value = if cfg!(debug_assertions) {
            "target/debug/cxlalloc-bench-coordinator"
        } else {
            "target/release/cxlalloc-bench-coordinator"
        }
    )]
    coordinator: PathBuf,

    #[arg(short, long, default_value = "result.ndjson")]
    output: PathBuf,

    #[command(subcommand)]
    experiment: Experiment,
}

impl Config {}

#[derive(Parser)]
enum Experiment {
    Ycsb(Box<Ycsb>),
    Memcached {
        #[arg(long, value_delimiter = ',', default_value = "10000000")]
        operation_count: Vec<u64>,

        #[arg(
            long,
            value_delimiter = ',',
            default_value = "\
            twitter/cluster12.000.parquet,\
            twitter/cluster15.000.parquet,\
            twitter/cluster31.000.parquet"
        )]
        trace: Vec<PathBuf>,
    },
    Mstress,
    Xmalloc {
        #[arg(long, value_delimiter = ',', default_value = "100")]
        limit: Vec<u64>,

        #[arg(long, value_delimiter = ',', default_value = "10000000")]
        operation_count: Vec<u64>,
    },
    ThreadTest {
        #[arg(long, value_delimiter = ',', default_value = "100")]
        iteration_count: Vec<u64>,

        #[arg(long, value_delimiter = ',', default_value = "100000")]
        operation_count: Vec<u64>,

        #[arg(long, value_delimiter = ',', default_value = "8")]
        object_size: Vec<usize>,
    },
}

#[derive(Parser)]
struct Ycsb {
    #[arg(long, value_delimiter = ',', default_value = "10000000")]
    record_count: Vec<usize>,

    #[arg(long, value_delimiter = ',', default_value = "1000000")]
    operation_count: Vec<usize>,

    #[arg(short, long, value_delimiter = ',', default_value = "linked")]
    index: Vec<Index>,

    // 2^25
    #[arg(long, value_delimiter = ',', default_value = "33554432")]
    index_len: Vec<usize>,

    #[arg(long, value_delimiter = ',', default_value = "false")]
    index_populate: Vec<bool>,

    #[command(subcommand)]
    workload: Workload,
}

#[derive(Parser)]
enum Workload {
    Load,
    D {
        #[arg(short, long, value_delimiter = ',', default_value = "0.05")]
        insert_proportion: Vec<f32>,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Config::parse();

    let mut out = File::options()
        .create(true)
        .append(true)
        .open(&cli.output)?;

    // Inefficient but easy to maintain
    let mut total = 0;
    cli.for_each(|config| cli.experiment.for_each(&config, |_| total += 1));

    let mut i = 0;
    cli.for_each(|config| {
        cli.experiment.for_each(&config, |config| {
            i += 1;
            cli.run(&config, i, total, &mut out).unwrap();
        })
    });

    Ok(())
}

impl Experiment {
    fn for_each<F: FnMut(cxlalloc_bench::Config)>(
        &self,
        config: &cxlalloc_bench::ConfigBuilder<
            cxlalloc_bench::config::SetAllocator<cxlalloc_bench::config::SetGlobal>,
        >,
        mut apply: F,
    ) {
        match self {
            Experiment::Memcached {
                operation_count,
                trace,
            } => {
                let index = Index::Linked;
                let populate = false;
                let len = 1 << 25;

                cartesian!(&operation_count, &trace)
                    .map(|(operation_count, trace)| {
                        config
                            .clone()
                            .benchmark(allocator_bench::benchmark::Config::Memcached(
                                allocator_bench::benchmark::memcached::Config::builder()
                                    .index(
                                        allocator_bench::index::Config::builder()
                                            .name(index.to_string())
                                            .populate(populate)
                                            .len(len)
                                            .build(),
                                    )
                                    .operation_count(*operation_count)
                                    .trace(trace.clone())
                                    .build(),
                            ))
                            .build()
                    })
                    .for_each(apply)
            }
            Experiment::Mstress => apply(
                config
                    .clone()
                    .benchmark(allocator_bench::benchmark::Config::Mstress(
                        allocator_bench::benchmark::Mstress::builder().build(),
                    ))
                    .build(),
            ),
            Experiment::Ycsb(ycsb) => match &ycsb.workload {
                Workload::Load => ycsb.for_each(|(index, record_count, operation_count)| {
                    apply(
                        config
                            .clone()
                            .benchmark(allocator_bench::benchmark::Config::YcsbLoad(
                                allocator_bench::benchmark::ycsb_load::Config::builder()
                                    .index(index.clone())
                                    .workload(
                                        ycsb::Workload::builder()
                                            .record_count(record_count)
                                            .operation_count(operation_count)
                                            .build(),
                                    )
                                    .build(),
                            ))
                            .build(),
                    )
                }),
                Workload::D { insert_proportion } => {
                    ycsb.for_each(|(index, record_count, operation_count)| {
                        insert_proportion
                            .iter()
                            .map(|insert_proportion| {
                                config
                                    .clone()
                                    .benchmark(allocator_bench::benchmark::Config::Ycsb(
                                        allocator_bench::benchmark::ycsb::Config::builder()
                                            .index(index.clone())
                                            .workload(ycsb::Workload {
                                                record_count,
                                                operation_count,
                                                insert_proportion: *insert_proportion,
                                                read_proportion: 1.0 - insert_proportion,
                                                ..ycsb::workload::D.clone()
                                            })
                                            .build(),
                                    ))
                                    .build()
                            })
                            .for_each(&mut apply)
                    })
                }
            },
            Experiment::Xmalloc {
                limit,
                operation_count,
            } => cartesian!(&limit, &operation_count)
                .filter_map(|(&limit, &operation_count)| {
                    let config = config
                        .clone()
                        .benchmark(allocator_bench::benchmark::Config::Xmalloc(
                            allocator_bench::benchmark::Xmalloc::builder()
                                .limit(limit)
                                .operation_count(operation_count)
                                .build(),
                        ))
                        .build();

                    // Only allow even process counts
                    match config.global.process_count & 1 {
                        0 => Some(config),
                        _ => None,
                    }
                })
                .for_each(apply),
            Experiment::ThreadTest {
                iteration_count,
                operation_count,
                object_size,
            } => cartesian!(&iteration_count, &operation_count, &object_size)
                .map(|(iteration_count, operation_count, object_size)| {
                    config
                        .clone()
                        .benchmark(allocator_bench::benchmark::Config::ThreadTest(
                            allocator_bench::benchmark::ThreadTest::builder()
                                .iteration_count(*iteration_count)
                                .operation_count(*operation_count)
                                .object_size(*object_size)
                                .build(),
                        ))
                        .build()
                })
                .for_each(apply),
        }
    }
}

impl Config {
    fn for_each<
        F: FnMut(
            cxlalloc_bench::ConfigBuilder<
                cxlalloc_bench::config::SetAllocator<cxlalloc_bench::config::SetGlobal>,
            >,
        ),
    >(
        &self,
        apply: F,
    ) {
        cartesian!(
            &self.process_count,
            &self.thread_count,
            &self.allocator,
            &self.allocator_config,
            &self.allocator_numa,
            &self.allocator_size,
            &self.allocator_populate,
        )
        .filter_map(
            |(process_count, thread_count, allocator, config, numa, size, populate)| {
                if thread_count % process_count != 0 {
                    return None;
                }

                Some(
                    cxlalloc_bench::Config::builder()
                        .global(allocator_bench::config::Global::new(
                            *process_count,
                            *thread_count,
                        ))
                        .allocator(
                            allocator_bench::allocator::Config::builder()
                                .name(allocator.to_string())
                                .numa(*numa)
                                .size(*size)
                                .populate(*populate)
                                .consistency(CONSISTENCY)
                                .inner(serde_json::from_str(config).unwrap())
                                .build(),
                        ),
                )
            },
        )
        .for_each(apply)
    }

    fn run(
        &self,
        config: &cxlalloc_bench::Config,
        index: usize,
        total: usize,
        out: &mut File,
    ) -> anyhow::Result<()> {
        const EMPTY: [String; 0] = [];

        eprintln!("{}/{}: {:?}", index + 1, total, config);

        let handle = duct::cmd(&self.coordinator, EMPTY)
            .stdin_bytes(serde_json::to_vec(&config)?)
            .stdout_file(out.try_clone()?)
            .start()?;
        let output = handle.wait()?;

        if !output.status.success() {
            return Err(anyhow!(
                "Command {:?} failed with status code {:?}",
                config,
                output.status,
            ));
        }

        Ok(())
    }
}

impl Ycsb {
    fn for_each<F: FnMut((allocator_bench::index::Config, usize, usize))>(&self, apply: F) {
        cartesian!(
            &self.index,
            &self.index_len,
            &self.index_populate,
            &self.record_count,
            &self.operation_count,
        )
        .map(|(index, len, populate, record_count, operation_count)| {
            (
                allocator_bench::index::Config::builder()
                    .name(index.to_string())
                    .populate(*populate)
                    .len(*len)
                    .build(),
                *record_count,
                *operation_count,
            )
        })
        .for_each(apply)
    }
}
