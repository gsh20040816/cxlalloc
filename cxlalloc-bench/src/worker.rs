use core::sync::atomic::Ordering;
use std::time::SystemTime;

use allocator_bench::benchmark;
use allocator_bench::index;
use bon::Builder;
use serde::Deserialize;
use serde::Serialize;

use crate::allocator::boost;
use crate::allocator::cxl_shm;
use crate::allocator::cxlalloc;
use crate::allocator::lightning;
use crate::allocator::mimalloc;
use crate::allocator::ralloc;
use crate::Allocator;
use crate::Index;

#[derive(Builder, Clone, Deserialize, Serialize)]
pub struct Config {
    pub config_process: allocator_bench::config::Process,
    pub config_allocator: allocator_bench::allocator::Config,
    pub config_benchmark: allocator_bench::benchmark::Config,

    pub allocator: Allocator,
    pub index: Index,
}

impl Config {
    pub fn run(self) {
        let _ = env_logger::Builder::from_default_env()
            .format(move |buffer, record| {
                use std::io::Write;

                use env_logger::fmt::style;

                let process_id = allocator_bench::PROCESS_ID.load(Ordering::Relaxed);
                let style_process = style::Ansi256Color::from(process_id as u8 + 1).on_default();

                // Color-code process ID if there is more than one process
                if allocator_bench::PROCESS_COUNT.load(Ordering::Relaxed) > 1 {
                    write!(buffer, "[{style_process}P{process_id:02}{style_process:#}]")?;
                }

                // Color-code thread ID
                match allocator_bench::THREAD_ID.get() {
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

    fn specialize_allocator(&self) {
        match self.allocator {
            Allocator::Boost => self.specialize_index::<boost::Backend>(),
            Allocator::Cxlalloc => self.specialize_index::<cxlalloc::Backend>(),
            Allocator::CxlShm => self.specialize_index::<cxl_shm::Backend>(),
            Allocator::Lightning => self.specialize_index::<lightning::Backend>(),
            Allocator::Mimalloc => self.specialize_index::<mimalloc::Backend>(),
            Allocator::Ralloc => self.specialize_index::<ralloc::Backend>(),
        }
    }

    fn specialize_index<B: allocator_bench::allocator::Backend>(&self) {
        match self.index {
            Index::Linear => self.specialize_benchmark::<B, index::LinearHashMap>(),
            Index::Linked => self.specialize_benchmark::<B, index::LinkedHashMap<B::Allocator>>(),
        }
    }

    fn specialize_benchmark<
        B: allocator_bench::allocator::Backend,
        I: allocator_bench::index::Index<B::Allocator>,
    >(
        &self,
    ) {
        match self.config_benchmark.clone() {
            benchmark::Config::Memcached(memcached) => self.run_benchmark::<B, _>(
                allocator_bench::benchmark::Memcached::<B::Allocator, I>::new(memcached),
            ),
            benchmark::Config::Mstress(mstress) => self.run_benchmark::<B, _>(mstress),
            benchmark::Config::ThreadTest(thread_test) => self.run_benchmark::<B, _>(thread_test),
            benchmark::Config::Ycsb(ycsb) => self.run_benchmark::<B, _>(
                allocator_bench::benchmark::Ycsb::<B::Allocator, I>::new(ycsb),
            ),
            benchmark::Config::YcsbLoad(ycsb_load) => self.run_benchmark::<B, _>(
                allocator_bench::benchmark::YcsbLoad::<B::Allocator, I>::new(ycsb_load),
            ),
            benchmark::Config::Xmalloc(xmalloc) => self.run_benchmark::<B, _>(xmalloc),
        }
    }

    fn run_benchmark<A: allocator_bench::allocator::Backend, B: benchmark::Benchmark<A>>(
        &self,
        benchmark: B,
    ) {
        benchmark.run_process(&self.config_process, &self.config_allocator)
    }
}
