use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

use anyhow::anyhow;
use cartesian::cartesian;
use cxlalloc_bench::allocator::Allocator;
use cxlalloc_bench::index;
use cxlalloc_bench::Index;
use serde::Deserialize;
use serde_inline_default::serde_inline_default;

#[serde_inline_default]
#[derive(Deserialize)]
struct Config {
    #[serde_inline_default(vec![1])]
    process_count: Vec<usize>,

    #[serde_inline_default(vec![1,2,4,8,16,32,40])]
    thread_count: Vec<usize>,

    #[serde_inline_default(vec![
        Allocator::cxlalloc(),
        Allocator::CxlShm,
        Allocator::Boost,
        Allocator::Lightning,
        Allocator::Mimalloc,
        Allocator::Ralloc,
    ])]
    allocator: Vec<Allocator>,

    #[serde(default)]
    allocator_config: cxlalloc_bench::allocator::Config,

    #[serde_inline_default(PathBuf::from(if cfg!(debug_assertions) {
            "target/debug/cxlalloc-bench-coordinator"
        } else {
            "target/release/cxlalloc-bench-coordinator"
        }
    ))]
    coordinator: PathBuf,

    #[serde_inline_default(PathBuf::from("result.ndjson"))]
    output: PathBuf,

    benchmark: Vec<Benchmark>,
}

impl Config {}

#[derive(Deserialize)]
enum Benchmark {
    KeyValue(KeyValue),
    Mstress,
    ThreadTest(ThreadTest),
    Xmalloc(Xmalloc),
}

#[serde_inline_default]
#[derive(Deserialize)]
struct KeyValue {
    #[serde_inline_default(vec![Index::Linked])]
    index: Vec<Index>,

    #[serde(default)]
    index_config: index::Config,

    benchmark: Vec<KeyValueBenchmark>,
}

#[derive(Deserialize)]
enum KeyValueBenchmark {
    Memcached(Memcached),
    Ycsb(Box<Ycsb>),
}

#[serde_inline_default]
#[derive(Deserialize)]
struct Memcached {
    #[serde_inline_default(vec![10_000_000])]
    operation_count: Vec<u64>,

    #[serde_inline_default(
        [
            "twitter/cluster12.000.parquet",
            "twitter/cluster15.000.parquet",
            "twitter/cluster31.000.parquet",
        ].into_iter().map(PathBuf::from).collect()
    )]
    trace: Vec<PathBuf>,
}

#[serde_inline_default]
#[derive(Deserialize)]
struct ThreadTest {
    #[serde_inline_default(vec![100])]
    iteration_count: Vec<u64>,

    #[serde_inline_default(vec![100_000])]
    operation_count: Vec<u64>,

    #[serde_inline_default(vec![8])]
    object_size: Vec<usize>,
}

#[serde_inline_default]
#[derive(Deserialize)]
struct Xmalloc {
    #[serde_inline_default(vec![100])]
    limit: Vec<u64>,

    #[serde_inline_default(vec![10_000_000])]
    operation_count: Vec<u64>,
}

#[serde_inline_default]
#[derive(Deserialize)]
struct Ycsb {
    #[serde_inline_default(vec![10_000_000])]
    record_count: Vec<usize>,

    #[serde_inline_default(vec![10_000_000])]
    operation_count: Vec<usize>,

    workload: Workload,
}

#[derive(Deserialize)]
enum Workload {
    Load,
    D(YcsbD),
}

#[serde_inline_default]
#[derive(Deserialize)]
struct YcsbD {
    #[serde_inline_default(vec![0.05])]
    insert_proportion: Vec<f32>,
}

fn main() -> anyhow::Result<()> {
    let r#in = std::env::args()
        .nth(1)
        .map(std::fs::read_to_string)
        .expect("Expected path to configuration file")?;

    let config = toml::from_str::<Config>(&r#in)?;

    let mut out = File::options()
        .create(true)
        .append(true)
        .open(&config.output)?;

    // Inefficient but easy to maintain
    let mut total = 0;
    config.for_each_cartesian(|global| {
        config
            .benchmark
            .iter()
            .for_each(|benchmark| benchmark.for_each_cartesian(global.clone(), |_| total += 1))
    });

    let mut i = 0;
    config.for_each_cartesian(|global| {
        config.benchmark.iter().for_each(|benchmark| {
            benchmark.for_each_cartesian(global.clone(), |benchmark| {
                i += 1;
                config.run(&benchmark, i, total, &mut out).unwrap();
            })
        })
    });

    Ok(())
}

impl Benchmark {
    fn for_each_cartesian<F: FnMut(cxlalloc_bench::Config)>(
        &self,
        config: cxlalloc_bench::ConfigBuilder<
            cxlalloc_bench::config::SetAllocator<cxlalloc_bench::config::SetGlobal>,
        >,
        mut apply: F,
    ) {
        match self {
            Benchmark::KeyValue(key_value) => key_value.for_each_cartesian(config, apply),
            Benchmark::Mstress => apply(
                config
                    .benchmark(allocator_bench::benchmark::Config::Mstress(
                        allocator_bench::benchmark::Mstress::builder().build(),
                    ))
                    .build(),
            ),
            Benchmark::Xmalloc(Xmalloc {
                limit,
                operation_count,
            }) => cartesian!(&limit, &operation_count)
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
            Benchmark::ThreadTest(ThreadTest {
                iteration_count,
                operation_count,
                object_size,
            }) => cartesian!(&iteration_count, &operation_count, &object_size)
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

impl KeyValue {
    fn for_each_cartesian<F: FnMut(cxlalloc_bench::Config)>(&self, config: Partial, mut apply: F) {
        self.index.iter().for_each(|index| {
            self.index_config.for_each_cartesian(*index, |index| {
                self.benchmark.iter().for_each(|benchmark| match benchmark {
                    KeyValueBenchmark::Memcached(Memcached {
                        operation_count,
                        trace,
                    }) => cartesian!(&operation_count, &trace)
                        .map(|(operation_count, trace)| {
                            config
                                .clone()
                                .benchmark(allocator_bench::benchmark::Config::Memcached(
                                    allocator_bench::benchmark::memcached::Config::builder()
                                        .index(index.clone())
                                        .operation_count(*operation_count)
                                        .trace(trace.clone())
                                        .build(),
                                ))
                                .build()
                        })
                        .for_each(&mut apply),
                    KeyValueBenchmark::Ycsb(ycsb) => {
                        ycsb.for_each_cartesian(config.clone(), index.clone(), &mut apply)
                    }
                })
            })
        })
    }
}

type Partial = cxlalloc_bench::ConfigBuilder<
    cxlalloc_bench::config::SetAllocator<cxlalloc_bench::config::SetGlobal>,
>;

impl Config {
    fn for_each_cartesian<
        F: FnMut(
            cxlalloc_bench::ConfigBuilder<
                cxlalloc_bench::config::SetAllocator<cxlalloc_bench::config::SetGlobal>,
            >,
        ),
    >(
        &self,
        mut apply: F,
    ) {
        cartesian!(&self.process_count, &self.thread_count)
            .filter(|(process_count, thread_count)| **thread_count % **process_count == 0)
            .map(|(process_count, thread_count)| {
                cxlalloc_bench::Config::builder().global(allocator_bench::config::Global::new(
                    *process_count,
                    *thread_count,
                ))
            })
            .for_each(|global_config| {
                self.allocator_config
                    .for_each_cartesian(|allocator_config| {
                        self.allocator.iter().for_each(|allocator| {
                            allocator.for_each_cartesian(allocator_config.clone(), |allocator| {
                                apply(global_config.clone().allocator(allocator))
                            })
                        })
                    })
            })
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
    fn for_each_cartesian<F: FnMut(cxlalloc_bench::Config)>(
        &self,
        config: Partial,
        index: allocator_bench::index::Config,
        mut apply: F,
    ) {
        cartesian!(&self.record_count, &self.operation_count,)
            .map(|(record_count, operation_count)| {
                ycsb::Workload::builder()
                    .record_count(*record_count)
                    .operation_count(*operation_count)
            })
            .for_each(|workload| match &self.workload {
                Workload::Load => apply(
                    config
                        .clone()
                        .benchmark(allocator_bench::benchmark::Config::YcsbLoad(
                            allocator_bench::benchmark::ycsb_load::Config::builder()
                                .index(index.clone())
                                .workload(
                                    workload.read_proportion(0.0).insert_proportion(1.0).build(),
                                )
                                .build(),
                        ))
                        .build(),
                ),
                Workload::D(YcsbD { insert_proportion }) => insert_proportion
                    .iter()
                    .map(|insert_proportion| {
                        config
                            .clone()
                            .benchmark(allocator_bench::benchmark::Config::Ycsb(
                                allocator_bench::benchmark::ycsb::Config::builder()
                                    .index(index.clone())
                                    .workload(
                                        workload
                                            .clone()
                                            .insert_proportion(*insert_proportion)
                                            .read_proportion(1.0 - insert_proportion)
                                            .update_proportion(0.0)
                                            .request_distribution(ycsb::RequestDistribution::Latest)
                                            .build(),
                                    )
                                    .build(),
                            ))
                            .build()
                    })
                    .for_each(&mut apply),
            })
    }
}
