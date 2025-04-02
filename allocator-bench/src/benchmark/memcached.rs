use core::any;
use core::cmp;
use core::marker::PhantomData;
use core::ops::Deref;
use std::fs::File;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use arrow_array::cast::AsArray as _;
use arrow_array::types::UInt64Type;
use bon::Builder;
use parquet::arrow::ProjectionMask;
use parquet::arrow::arrow_reader::ArrowReaderMetadata;
use parquet::arrow::arrow_reader::ArrowReaderOptions;
use parquet::arrow::arrow_reader::ParquetRecordBatchReader;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use parquet::basic;
use parquet::schema::types::SchemaDescriptor;
use parquet::schema::types::Type;
use serde::Deserialize;
use serde::Serialize;

use crate::Allocator;
use crate::Index;
use crate::allocator;
use crate::allocator::Backend;
use crate::benchmark;
use crate::config;
use crate::index;

#[derive(Builder, Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    index: index::Config,

    operation_count: u64,

    trace: PathBuf,
}

pub struct Memcached<A: Allocator, I: Index<A>> {
    config: Config,
    _index: PhantomData<fn() -> (A, I)>,
}

impl<A: Allocator, I: Index<A>> Memcached<A, I> {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            _index: PhantomData,
        }
    }
}

impl<A: Allocator, I: Index<A>> Deref for Memcached<A, I> {
    type Target = Config;
    fn deref(&self) -> &Self::Target {
        &self.config
    }
}

// HACK: CXL-SHM doesn't support allocations larger than 1KiB (1_000B data + 24B header)
#[expect(unused)]
const MAX_SIZE: usize = 1_000;

pub struct Global<I> {
    index: I,

    mask: ProjectionMask,

    metadata: ArrowReaderMetadata,
}

unsafe impl<I> Sync for Global<I> {}

pub struct Worker {
    reader: ParquetRecordBatchReader,
}

#[derive(Serialize)]
pub struct Output {
    time: u128,
    throughput: u64,
}

impl<B: Backend, I: Index<B::Allocator>> benchmark::Benchmark<B> for Memcached<B::Allocator, I> {
    const NAME: &str = "/mc";
    type StateGlobal = Global<I>;
    type StateProcess = ();
    type StateCoordinator = ();
    type StateWorker = Worker;

    type OutputWorker = u128;
    type OutputCoordinator = u64;
    type OutputProcess = Output;

    fn setup_global(
        &self,
        config: &config::Process,
        allocator: &allocator::Config,
    ) -> Self::StateGlobal {
        let file = File::open(&self.trace).unwrap();
        let schema = SchemaDescriptor::new(Arc::new(
            Type::group_type_builder("trace_schema")
                .with_fields(vec![
                    Arc::new(
                        Type::primitive_type_builder("timestamp", basic::Type::INT64)
                            .with_converted_type(basic::ConvertedType::UINT_64)
                            .build()
                            .unwrap(),
                    ),
                    Arc::new(
                        Type::primitive_type_builder("key_value", basic::Type::BYTE_ARRAY)
                            .with_logical_type(Some(basic::LogicalType::String))
                            .build()
                            .unwrap(),
                    ),
                    Arc::new(
                        Type::primitive_type_builder("key_size", basic::Type::INT64)
                            .with_converted_type(basic::ConvertedType::UINT_64)
                            .build()
                            .unwrap(),
                    ),
                    Arc::new(
                        Type::primitive_type_builder("value_size", basic::Type::INT64)
                            .with_converted_type(basic::ConvertedType::UINT_64)
                            .build()
                            .unwrap(),
                    ),
                    Arc::new(
                        Type::primitive_type_builder("client_id", basic::Type::INT64)
                            .with_converted_type(basic::ConvertedType::UINT_64)
                            .build()
                            .unwrap(),
                    ),
                    Arc::new(
                        Type::primitive_type_builder("operation", basic::Type::BYTE_ARRAY)
                            .with_logical_type(Some(basic::LogicalType::String))
                            .build()
                            .unwrap(),
                    ),
                    Arc::new(
                        Type::primitive_type_builder("ttl", basic::Type::INT64)
                            .with_converted_type(basic::ConvertedType::UINT_64)
                            .build()
                            .unwrap(),
                    ),
                ])
                .build()
                .unwrap(),
        ));

        let mask = ProjectionMask::columns(&schema, ["key_value", "value_size", "operation"]);

        let metadata =
            ArrowReaderMetadata::load(&file, ArrowReaderOptions::new().with_page_index(true))
                .unwrap();
        Global {
            index: I::new(
                Some(allocator.numa),
                self.index.len,
                config.is_leader(),
                self.index.populate,
                config.thread_count,
            )
            .unwrap(),
            mask,
            metadata,
        }
    }

    fn setup_process(
        &self,
        _config: &config::Process,
        _allocator: &allocator::Config,
    ) -> Self::StateProcess {
    }

    fn setup_coordinator(
        &self,
        _config: &config::Process,
        _global: &Self::StateGlobal,
        (): &Self::StateProcess,
    ) -> Self::StateCoordinator {
    }

    fn setup_worker(
        &self,
        config: &config::Thread,
        global: &Self::StateGlobal,
        (): &Self::StateProcess,
        _allocator: &mut B::Allocator,
    ) -> Self::StateWorker {
        let limit = self.operation_count as usize / config.thread_count;
        let offset = limit * config.thread_id;
        let file = File::open(&self.trace).unwrap();
        let reader =
            ParquetRecordBatchReaderBuilder::new_with_metadata(file, global.metadata.clone())
                .with_offset(offset)
                .with_limit(limit)
                .with_projection(global.mask.clone())
                .build()
                .unwrap();
        Worker { reader }
    }

    fn run_coordinator(
        &self,
        _config: &config::Process,
        _global: &Self::StateGlobal,
        (): &Self::StateProcess,
        _coordinator: &mut Self::StateCoordinator,
    ) -> Self::OutputCoordinator {
        self.operation_count
    }

    fn run_worker(
        &self,
        config: &config::Thread,
        global: &Self::StateGlobal,
        (): &Self::StateProcess,
        worker: &mut Self::StateWorker,
        allocator: &mut B::Allocator,
    ) -> Self::OutputWorker {
        let start = Instant::now();

        for batch in &mut worker.reader {
            let batch = batch.unwrap();
            for ((key, value_size), operation) in batch
                .column(0)
                .as_string::<i32>()
                .iter()
                .flatten()
                .zip(
                    batch
                        .column(1)
                        .as_primitive::<UInt64Type>()
                        .iter()
                        .flatten(),
                )
                .zip(batch.column(2).as_string::<i32>().iter().flatten())
            {
                match operation {
                    "get" => {
                        global
                            .index
                            .get(config.thread_id, allocator, key.as_bytes(), |pointer| {
                                if pointer.is_null() {
                                    return;
                                }

                                let size = unsafe { pointer.cast::<u64>().read() };
                                let value = unsafe {
                                    core::slice::from_raw_parts(pointer.byte_add(8), size as usize)
                                };

                                assert!(value.iter().all(|byte| *byte == 0xff));
                            });
                    }
                    "set" if value_size == 0 => {
                        global
                            .index
                            .insert(config.thread_id, allocator, key.as_bytes(), 0, |_| ())
                    }
                    "set" => {
                        let value_size = if any::type_name::<B>().contains("cxl_shm") {
                            cmp::min(value_size, 992)
                        } else {
                            value_size
                        };

                        global.index.insert(
                            config.thread_id,
                            allocator,
                            key.as_bytes(),
                            8 + value_size as usize,
                            |pointer| unsafe {
                                pointer.cast::<u64>().write(value_size);
                                libc::memset(pointer.byte_add(8).cast(), 0xff, value_size as usize);
                            },
                        )
                    }
                    _ => unreachable!(),
                }
            }
        }

        start.elapsed().as_nanos()
    }

    fn teardown_global(&self, config: &config::Process, mut global: Self::StateGlobal) {
        if !config.is_leader() {
            return;
        }

        global.index.unlink().unwrap();
    }

    fn aggregate(
        operation_count: Self::OutputCoordinator,
        workers: Vec<Self::OutputWorker>,
    ) -> Self::OutputProcess {
        let total = workers.iter().sum::<u128>();
        let time = total / workers.len() as u128;
        let throughput = (operation_count as f64 / time as f64 * 1e9) as u64;
        Output { time, throughput }
    }
}
