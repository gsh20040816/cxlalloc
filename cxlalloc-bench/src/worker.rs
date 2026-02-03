use core::sync::atomic::Ordering;
use std::time::SystemTime;

use bon::Builder;
use serde::Deserialize;
use serde::Serialize;
use shm_bench::benchmark;
use shm_bench::index;

#[derive(Builder, Clone, Deserialize, Serialize)]
pub struct Config {
    pub date: u128,
    pub process: shm_bench::config::Process,
    pub allocator: shm_bench::allocator::Config<serde_json::Value>,
    pub benchmark: shm_bench::benchmark::Config,
}

impl Config {
    pub fn run(self) -> anyhow::Result<()> {
        let _ = env_logger::Builder::from_default_env()
            .format(move |buffer, record| {
                use std::io::Write;

                use env_logger::fmt::style;

                let process_id = shm_bench::PROCESS_ID.load(Ordering::Relaxed);
                let style_process = style::Ansi256Color::from(process_id as u8 + 1).on_default();

                // Color-code process ID if there is more than one process
                if shm_bench::PROCESS_COUNT.load(Ordering::Relaxed) > 1 {
                    write!(buffer, "[{style_process}P{process_id:02}{style_process:#}]")?;
                }

                // Color-code thread ID
                match shm_bench::THREAD_ID.get() {
                    None => {
                        write!(buffer, "[{style_process}C{process_id:02}{style_process:#}]")?;
                    }
                    Some(thread_id) => {
                        let style_thread =
                            style::Ansi256Color::from(thread_id as u8 + 17).on_default();
                        write!(buffer, "[{style_thread}T{thread_id:02}{style_thread:#}]")?;
                    }
                }

                // Abbreviated log level
                let level = match record.level() {
                    log::Level::Error => "E",
                    log::Level::Warn => "W",
                    log::Level::Info => "I",
                    log::Level::Debug => "D",
                    log::Level::Trace => "T",
                };
                let style_level = buffer.default_level_style(record.level());
                write!(buffer, "[{style_level}{level}{style_level:#}]")?;

                let time = SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|duration| duration.as_secs())
                    .unwrap_or(0);
                write!(buffer, "[{time:016}]")?;

                writeln!(buffer, "[{}]: {}", record.target(), record.args())?;
                buffer.flush()?;
                Ok(())
            })
            .try_init();

        self.specialize_allocator()
    }

    fn specialize_allocator(&self) -> anyhow::Result<()> {
        match self.allocator.name.as_str() {
            #[cfg(feature = "allocator-boost")]
            "boost" => self.specialize_benchmark::<crate::allocator::boost::Backend>(),
            #[cfg(feature = "allocator-cxlalloc")]
            "cxlalloc" => self.specialize_benchmark::<crate::allocator::cxlalloc::Backend>(),
            #[cfg(feature = "allocator-cxl-shm")]
            "cxl_shm" => self.specialize_benchmark::<crate::allocator::cxl_shm::Backend>(),
            #[cfg(feature = "allocator-lightning")]
            "lightning" => self.specialize_benchmark::<crate::allocator::lightning::Backend>(),
            #[cfg(feature = "allocator-mimalloc")]
            "mimalloc" => self.specialize_benchmark::<crate::allocator::mimalloc::Backend>(),
            #[cfg(feature = "allocator-ralloc")]
            "ralloc" => self.specialize_benchmark::<crate::allocator::ralloc::Backend>(),
            allocator => panic!("Unrecognized allocator: {allocator}"),
        }
    }

    fn specialize_benchmark<B: shm_bench::allocator::Backend>(&self) -> anyhow::Result<()> {
        type Measure<B> = shm_bench::measure::time::Backend<B>;

        // FIXME: figure out how to conditionally specialize index
        match self.benchmark.clone() {
            benchmark::Config::Memcached(memcached) => {
                assert_eq!(memcached.index.name, "linked");
                self.run_benchmark::<Measure<B>, _>(shm_bench::index::Capture::<
                    _,
                    index::LinkedHashMap<_>,
                >::new(memcached))
            }
            benchmark::Config::Mstress(mstress) => self.run_benchmark::<Measure<B>, _>(mstress),
            benchmark::Config::ThreadTest(thread_test) => {
                self.run_benchmark::<Measure<B>, _>(thread_test)
            }
            benchmark::Config::YcsbRun(ycsb) => {
                assert_eq!(ycsb.index.name, "linked");
                self.run_benchmark::<Measure<B>, _>(shm_bench::index::Capture::<
                    _,
                    index::LinkedHashMap<_>,
                >::new(ycsb))
            }
            benchmark::Config::YcsbLoad(ycsb) => {
                assert_eq!(ycsb.index.name, "linked");
                self.run_benchmark::<Measure<B>, _>(shm_bench::index::Capture::<
                    _,
                    index::LinkedHashMap<_>,
                >::new(
                    shm_bench::benchmark::ycsb_load::Config(ycsb)
                ))
            }
            benchmark::Config::Xmalloc(xmalloc) => self.run_benchmark::<Measure<B>, _>(xmalloc),
        }
    }

    fn run_benchmark<A: shm_bench::allocator::Backend, B: benchmark::Benchmark<A>>(
        &self,
        benchmark: B,
    ) -> anyhow::Result<()> {
        shm_bench::benchmark::run(
            &benchmark,
            self.date,
            &self.process,
            &self.allocator.map(|value| {
                serde_json::from_value(match value {
                    // The `flatten` attribute on `shm_bench::allocator::Config`
                    // causes us to parse `null` as an empty object, but we need null.
                    serde_json::Value::Object(object)
                        if std::any::type_name::<A::Config>() == "()" && object.is_empty() =>
                    {
                        serde_json::Value::Null
                    }
                    value => value.clone(),
                })
                .unwrap()
            }),
        )
    }
}
