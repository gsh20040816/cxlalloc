use core::cmp;
use core::iter;
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
struct Cli {
    #[arg(
        short,
        long,
        value_delimiter = ',',
        default_value = "cxlalloc,cxl-shm,boost,lightning,mimalloc,ralloc"
    )]
    allocator: Vec<Allocator>,

    #[arg(short, long, value_delimiter = ',', default_value = "1")]
    process_count: Vec<usize>,

    #[arg(short, long, value_delimiter = ',', default_value = "1,2,4,8,16,32,40")]
    thread_count: Vec<usize>,

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

impl Cli {
    fn collect(
        &self,
    ) -> Vec<(
        allocator_bench::config::Global,
        Allocator,
        allocator_bench::allocator::Config,
    )> {
        cartesian!(
            &self.process_count,
            &self.thread_count,
            &self.allocator,
            &self.allocator_numa,
            &self.allocator_size,
            &self.allocator_populate,
        )
        .filter_map(
            |(process_count, thread_count, allocator, numa, size, populate)| {
                if thread_count % process_count != 0 {
                    return None;
                }

                Some((
                    allocator_bench::config::Global::new(*process_count, *thread_count),
                    *allocator,
                    allocator_bench::allocator::Config::builder()
                        .numa(*numa)
                        .size(*size)
                        .populate(*populate)
                        .consistency(CONSISTENCY)
                        .build(),
                ))
            },
        )
        .collect()
    }
}

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
        object_count: Vec<u64>,

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

impl Ycsb {
    fn collect(&self) -> Vec<(Index, allocator_bench::index::Config, usize, usize)> {
        cartesian!(
            &self.index,
            &self.index_len,
            &self.index_populate,
            &self.record_count,
            &self.operation_count,
        )
        .map(|(index, len, populate, record_count, operation_count)| {
            (
                *index,
                allocator_bench::index::Config::builder()
                    .populate(*populate)
                    .len(*len)
                    .build(),
                *record_count,
                *operation_count,
            )
        })
        .collect()
    }
}

#[derive(Parser)]
enum Workload {
    Load,
    D {
        #[arg(long, default_value_t = 10)]
        time: u64,

        #[arg(long, value_delimiter = ',')]
        throughput: Vec<u64>,

        #[arg(short, long, value_delimiter = ',', default_value = "0.05")]
        insert_proportion: Vec<f32>,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let mut out = File::options()
        .create(true)
        .append(true)
        .open(&cli.output)?;

    let config = cli.collect();

    match &cli.experiment {
        Experiment::Memcached {
            operation_count,
            trace,
        } => {
            let total = config.len() * trace.len();
            for (i, ((config_global, allocator, config_allocator), operation_count, trace)) in
                cartesian!(&config, &operation_count, &trace).enumerate()
            {
                let config = cxlalloc_bench::Config::builder()
                    .allocator(*allocator)
                    .index(Index::Linked)
                    .config_allocator(*config_allocator)
                    .config_global(*config_global)
                    .config_benchmark(allocator_bench::benchmark::Config::Memcached(
                        allocator_bench::benchmark::memcached::Config::builder()
                            .index(
                                allocator_bench::index::Config::builder()
                                    .populate(false)
                                    .len(1 << 25)
                                    .build(),
                            )
                            .operation_count(*operation_count)
                            .trace(trace.clone())
                            .build(),
                    ))
                    .build();

                cli.run(&config, i, total, &mut out)?;
            }
        }
        Experiment::Mstress => {
            let total = config.len();
            for (i, (config_global, allocator, config_allocator)) in config.into_iter().enumerate()
            {
                let config = cxlalloc_bench::Config::builder()
                    .allocator(allocator)
                    .index(Index::Linked)
                    .config_allocator(config_allocator)
                    .config_global(config_global)
                    .config_benchmark(allocator_bench::benchmark::Config::Mstress(
                        allocator_bench::benchmark::Mstress::builder().build(),
                    ))
                    .build();

                cli.run(&config, i, total, &mut out)?;
            }
        }
        Experiment::Ycsb(ycsb) => {
            let config_ycsb = ycsb.collect();
            let total = config.len() * config_ycsb.len();

            for (
                i,
                (
                    (config_global, allocator, config_allocator),
                    (index, config_index, record_count, operation_count),
                ),
            ) in cartesian!(&config, &config_ycsb).enumerate()
            {
                let config = || {
                    cxlalloc_bench::Config::builder()
                        .allocator(*allocator)
                        .index(*index)
                        .config_allocator(*config_allocator)
                        .config_global(*config_global)
                };

                match &ycsb.workload {
                    Workload::Load => {
                        let config = config()
                            .config_benchmark(allocator_bench::benchmark::Config::YcsbLoad(
                                allocator_bench::benchmark::ycsb_load::Config::builder()
                                    .index(config_index.clone())
                                    .workload(
                                        ycsb::Workload::builder()
                                            .record_count(*record_count)
                                            .operation_count(*operation_count)
                                            .build(),
                                    )
                                    .build(),
                            ))
                            .build();

                        cli.run(&config, i, total, &mut out)?;
                    }
                    Workload::D {
                        time,
                        throughput,
                        insert_proportion,
                    } => {
                        let partial = cmp::max(throughput.len(), 1) * insert_proportion.len();
                        let total = total * partial;
                        let throughput = match throughput.is_empty() {
                            true => {
                                Box::new(iter::once(None)) as Box<dyn Iterator<Item = Option<_>>>
                            }
                            false => Box::new(throughput.iter().map(Some)),
                        };

                        for (j, (throughput, insert_proportion)) in
                            cartesian!(throughput, &insert_proportion).enumerate()
                        {
                            let config = config()
                                .config_benchmark(allocator_bench::benchmark::Config::Ycsb(
                                    allocator_bench::benchmark::ycsb::Config::builder()
                                        .index(config_index.clone())
                                        .workload(ycsb::Workload {
                                            record_count: *record_count,
                                            operation_count: *operation_count,
                                            insert_proportion: *insert_proportion,
                                            read_proportion: 1.0 - insert_proportion,
                                            ..ycsb::workload::D.clone()
                                        })
                                        .maybe_throughput(throughput.copied())
                                        .time(*time)
                                        .build(),
                                ))
                                .build();

                            cli.run(&config, i * partial + j, total, &mut out)?;
                        }
                    }
                }
            }
        }
        Experiment::Xmalloc {
            limit,
            operation_count,
        } => {
            let total = config.len() * limit.len() * operation_count.len();

            for (index, ((config_global, allocator, config_allocator), &limit, &operation_count)) in
                cartesian!(config.into_iter(), &limit, &operation_count).enumerate()
            {
                cli.run(
                    &cxlalloc_bench::Config::builder()
                        .allocator(allocator)
                        .index(Index::Linear)
                        .config_global(config_global)
                        .config_allocator(config_allocator)
                        .config_benchmark(allocator_bench::benchmark::Config::Xmalloc(
                            allocator_bench::benchmark::Xmalloc::builder()
                                .limit(limit)
                                .operation_count(operation_count)
                                .build(),
                        ))
                        .build(),
                    index,
                    total,
                    &mut out,
                )?;
            }
        }
        Experiment::ThreadTest {
            iteration_count,
            object_count,
            object_size,
        } => {
            let total =
                config.len() * iteration_count.len() * object_count.len() * object_size.len();

            for (
                index,
                (
                    (config_global, allocator, config_allocator),
                    iteration_count,
                    object_count,
                    object_size,
                ),
            ) in cartesian!(config, &iteration_count, &object_count, &object_size).enumerate()
            {
                cli.run(
                    &cxlalloc_bench::Config::builder()
                        .allocator(allocator)
                        .index(Index::Linear)
                        .config_global(config_global)
                        .config_allocator(config_allocator)
                        .config_benchmark(allocator_bench::benchmark::Config::ThreadTest(
                            allocator_bench::benchmark::ThreadTest::builder()
                                .iteration_count(*iteration_count)
                                .object_count(*object_count)
                                .object_size(*object_size)
                                .build(),
                        ))
                        .build(),
                    index,
                    total,
                    &mut out,
                )?;
            }
        }
    }

    Ok(())
}

impl Cli {
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
