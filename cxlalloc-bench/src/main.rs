use core::cmp;
use core::iter;
use std::fs::File;
use std::io::Write as _;
use std::path::PathBuf;

use allocator_bench::allocator::Consistency;
use anyhow::anyhow;
use cartesian::cartesian;
use cartesian::TuplePrepend as _;
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
        default_value = "cxlalloc,cxl-shm,boost,lightning"
    )]
    allocator: Vec<Allocator>,

    #[arg(short, long, value_delimiter = ',', default_value = "1")]
    process_count: Vec<usize>,

    #[arg(short, long, value_delimiter = ',', default_value = "40")]
    thread_total: Vec<usize>,

    #[arg(long, value_delimiter = ',', default_value = "1")]
    allocator_numa: Vec<usize>,

    // 2^35
    #[arg(long, value_delimiter = ',', default_value = "34359738368")]
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
            self.process_count.iter(),
            self.thread_total.iter(),
            self.allocator.iter(),
            self.allocator_numa.iter(),
            self.allocator_size.iter(),
            self.allocator_populate.iter()
        )
        .filter_map(
            |(process_count, thread_total, allocator, numa, size, populate)| {
                if thread_total % process_count != 0 {
                    return None;
                }

                Some((
                    allocator_bench::config::Global::builder()
                        .process_count(*process_count)
                        .thread_count(thread_total / process_count)
                        .build(),
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

        trace: Vec<PathBuf>,
    },
    Mstress,
    Xmalloc,
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
            self.index.iter(),
            self.index_len.iter(),
            self.index_populate.iter(),
            self.record_count.iter(),
            self.operation_count.iter()
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

    let config = cli.collect();

    match &cli.experiment {
        Experiment::Memcached {
            operation_count,
            trace,
        } => {
            let total = config.len() * trace.len();
            for (i, ((config_global, allocator, config_allocator), operation_count, trace)) in
                cartesian!(config.iter(), operation_count.iter(), trace.iter()).enumerate()
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
            ) in cartesian!(config.iter(), config_ycsb.iter()).enumerate()
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
                            cartesian!(throughput, insert_proportion.iter()).enumerate()
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
        Experiment::Xmalloc => {
            let total = config.len();

            for (index, (config_global, allocator, config_allocator)) in
                config.into_iter().enumerate()
            {
                cli.run(
                    &cxlalloc_bench::Config::builder()
                        .allocator(allocator)
                        .index(Index::Linear)
                        .config_global(config_global)
                        .config_allocator(config_allocator)
                        .config_benchmark(allocator_bench::benchmark::Config::Xmalloc(
                            allocator_bench::benchmark::Xmalloc::builder().build(),
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
            ) in cartesian!(
                config.into_iter(),
                iteration_count.iter(),
                object_count.iter(),
                object_size.iter()
            )
            .enumerate()
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

        out.write_all(b"\n")?;

        Ok(())
    }
}
