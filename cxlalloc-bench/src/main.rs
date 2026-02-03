use std::fs::File;
use std::path::Path;
use std::path::PathBuf;

use anyhow::anyhow;
use cartesian::cartesian;
use cxlalloc_bench::allocator::Allocator;
use cxlalloc_bench::index;
use cxlalloc_bench::Index;
use serde::Deserialize;
use serde_inline_default::serde_inline_default;
use shm::Numa;
use ycsb::RequestDistribution;

#[serde_inline_default]
#[derive(Deserialize)]
struct Config {
    /// Skip this many configurations
    #[serde_inline_default(0)]
    skip: usize,

    /// Repeat this many times
    #[serde_inline_default(1)]
    repeat: usize,

    #[serde_inline_default(PathBuf::from(if cfg!(debug_assertions) {
            "target/debug/cxlalloc-bench-coordinator"
        } else {
            "target/release/cxlalloc-bench-coordinator"
        }
    ))]
    coordinator: PathBuf,

    experiment: Vec<Experiment>,
}

#[serde_inline_default]
#[derive(Deserialize)]
struct Experiment {
    #[serde_inline_default(vec![1])]
    process_count: Vec<usize>,

    #[serde_inline_default(vec![1,2,4,8,16,32,40])]
    thread_count: Vec<usize>,

    /// Global NUMA policy
    #[serde_inline_default(vec![Some(Numa::Bind { node: 0 })])]
    numa: Vec<Option<Numa>>,

    #[serde_inline_default(vec![
        #[cfg(feature = "allocator-cxlalloc")]
        Allocator::cxlalloc(),
        #[cfg(feature = "allocator-cxl-shm")]
        Allocator::CxlShm,
        #[cfg(feature = "allocator-boost")]
        Allocator::Boost,
        #[cfg(feature = "allocator-lightning")]
        Allocator::Lightning,
        #[cfg(feature = "allocator-mimalloc")]
        Allocator::mimalloc(),
        #[cfg(feature = "allocator-ralloc")]
        Allocator::Ralloc,
    ])]
    allocator: Vec<Allocator>,

    #[serde(default)]
    allocator_config: cxlalloc_bench::allocator::Config,

    #[serde_inline_default(PathBuf::from("result.ndjson"))]
    output: PathBuf,

    benchmark: Vec<Benchmark>,
}

impl Config {}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
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

    workload: Vec<KeyValueWorkload>,
}

#[derive(Deserialize)]
#[serde(tag = "name", rename_all = "snake_case")]
enum KeyValueWorkload {
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

    #[serde_inline_default(vec![120])]
    batch_count: Vec<u64>,

    #[serde_inline_default(vec![10_000_000])]
    operation_count: Vec<u64>,

    #[serde_inline_default(vec![false])]
    huge: Vec<bool>,
}

#[serde_inline_default]
#[derive(Deserialize)]
struct Ycsb {
    #[serde_inline_default(vec![10_000_000])]
    record_count: Vec<usize>,

    #[serde_inline_default(vec![10_000_000])]
    operation_count: Vec<usize>,

    mix: Vec<Workload>,
}

#[derive(Deserialize)]
#[serde(tag = "phase", rename_all = "snake_case")]
enum Workload {
    Load,
    Run(Run),
}

#[derive(Deserialize)]
struct Run {
    #[serde(default)]
    read: f32,
    #[serde(default)]
    insert: f32,
    #[serde(default)]
    delete: f32,
    distribution: RequestDistribution,
}

fn main() -> anyhow::Result<()> {
    let r#in = std::env::args()
        .nth(1)
        .map(std::fs::read_to_string)
        .expect("Expected path to configuration file")?;

    let config = toml::from_str::<Config>(&r#in)?;

    // Inefficient but easy to maintain
    let mut partial = 0;
    for experiment in &config.experiment {
        if experiment.output.exists() {
            eprintln!("[WARN]: output file {:?} exists", experiment.output);
        }

        experiment.for_each_cartesian(|global| {
            experiment.benchmark.iter().for_each(|benchmark| {
                benchmark.for_each_cartesian(global.clone(), |config| {
                    if !config.skip() {
                        partial += 1
                    }
                })
            })
        });
    }

    let mut i = 0;
    let coordinator = &config.coordinator;
    let skip = config.skip;
    let repeat = config.repeat;

    for j in 0..config.repeat {
        for experiment in &config.experiment {
            let mut out = File::options()
                .create(true)
                .append(true)
                .open(&experiment.output)?;

            experiment.for_each_cartesian(|global| {
                experiment.benchmark.iter().for_each(|benchmark| {
                    benchmark.for_each_cartesian(global.clone(), |config| {
                        if config.skip() {
                            return;
                        }

                        if i >= skip {
                            experiment
                                .run(
                                    coordinator,
                                    &config,
                                    i % partial,
                                    partial,
                                    j,
                                    repeat,
                                    &mut out,
                                )
                                .unwrap();
                        }

                        i += 1;
                    })
                })
            });
        }
    }

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
                    .benchmark(shm_bench::benchmark::Config::Mstress(
                        shm_bench::benchmark::Mstress::builder().build(),
                    ))
                    .build(),
            ),
            Benchmark::Xmalloc(Xmalloc {
                limit,
                batch_count,
                operation_count,
                huge,
            }) => cartesian!(limit, batch_count, operation_count, huge)
                .map(|(&limit, &batch_count, &operation_count, &huge)| {
                    config
                        .clone()
                        .benchmark(shm_bench::benchmark::Config::Xmalloc(
                            shm_bench::benchmark::Xmalloc::builder()
                                .limit(limit)
                                .batch_count(batch_count)
                                .operation_count(operation_count)
                                .huge(huge)
                                .build(),
                        ))
                        .build()
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
                        .benchmark(shm_bench::benchmark::Config::ThreadTest(
                            shm_bench::benchmark::ThreadTest::builder()
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
                self.workload.iter().for_each(|benchmark| match benchmark {
                    KeyValueWorkload::Memcached(Memcached {
                        operation_count,
                        trace,
                    }) => cartesian!(&operation_count, &trace)
                        .map(|(operation_count, trace)| {
                            config
                                .clone()
                                .benchmark(shm_bench::benchmark::Config::Memcached(
                                    shm_bench::benchmark::memcached::Config::builder()
                                        .index(index.clone())
                                        .operation_count(*operation_count)
                                        .trace(trace.clone())
                                        .build(),
                                ))
                                .build()
                        })
                        .for_each(&mut apply),
                    KeyValueWorkload::Ycsb(ycsb) => {
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

impl Experiment {
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
        cartesian!(&self.process_count, &self.thread_count, &self.numa)
            .map(|(process_count, thread_count, numa)| {
                if process_count > thread_count {
                    (thread_count, thread_count, numa)
                } else {
                    (process_count, thread_count, numa)
                }
            })
            .filter_map(|(process_count, thread_count, numa)| {
                shm_bench::config::Global::builder()
                    .process_count(*process_count)
                    .thread_count(*thread_count)
                    .maybe_numa(numa.clone())
                    .build()
                    .map(|global| cxlalloc_bench::Config::builder().global(global))
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

    #[expect(clippy::too_many_arguments)]
    fn run(
        &self,
        coordinator: &Path,
        config: &cxlalloc_bench::Config,
        index_config: usize,
        count_config: usize,

        index_repeat: usize,
        count_repeat: usize,

        out: &mut File,
    ) -> anyhow::Result<()> {
        const EMPTY: [String; 0] = [];

        eprintln!(
            "{}/{} ({} / {}): {:?}",
            index_config + 1,
            count_config,
            index_repeat + 1,
            count_repeat,
            config
        );

        let handle = duct::cmd(coordinator, EMPTY)
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
        index: shm_bench::index::Config,
        mut apply: F,
    ) {
        cartesian!(&self.record_count, &self.operation_count, &self.mix)
            .map(|(record_count, operation_count, workload)| {
                let partial = ycsb::Workload::builder()
                    .record_count(*record_count)
                    .operation_count(*operation_count);

                let config = shm_bench::benchmark::ycsb_run::Config::builder().index(index.clone());

                match workload {
                    Workload::Load => shm_bench::benchmark::Config::YcsbLoad(
                        config
                            .workload(partial.read_proportion(0.0).insert_proportion(1.0).build())
                            .build(),
                    ),
                    Workload::Run(Run {
                        insert,
                        read,
                        delete,
                        distribution,
                    }) => shm_bench::benchmark::Config::YcsbRun(
                        config
                            .workload(
                                partial
                                    .insert_proportion(*insert)
                                    .read_proportion(*read)
                                    .update_proportion(0.0)
                                    .delete_proportion(*delete)
                                    .request_distribution(*distribution)
                                    .build(),
                            )
                            .build(),
                    ),
                }
            })
            .map(|benchmark| config.clone().benchmark(benchmark).build())
            .for_each(&mut apply)
    }
}
