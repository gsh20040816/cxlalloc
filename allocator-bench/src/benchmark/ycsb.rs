use core::marker::PhantomData;
use core::ops::Deref;
use core::sync::atomic::Ordering;
use core::time::Duration;
use std::sync::Mutex;
use std::time::Instant;

use bon::Builder;
use crossbeam_channel as mpmc;
use crossbeam_channel::RecvError;
use crossbeam_channel::TryRecvError;
use hdrhistogram::Histogram;
use rand::SeedableRng;
use rand::rngs::SmallRng;
use serde::Deserialize;
use serde::Serialize;
use shm::Shm;
use spin_sleep::SpinSleeper;
use spin_sleep::SpinStrategy;

use crate::Allocator;
use crate::Index;
use crate::allocator;
use crate::allocator::Backend;
use crate::benchmark;
use crate::config;
use crate::index;

use super::ycsb_load::Field;
use super::ycsb_load::Record;
use super::ycsb_load::insert;
use super::ycsb_load::load;

#[derive(Builder, Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    index: index::Config,

    throughput: Option<u64>,

    time: u64,

    #[serde(flatten)]
    workload: ycsb::Workload,
}

pub struct Ycsb<A: Allocator, I: Index<A>> {
    config: Config,
    _index: PhantomData<fn() -> (A, I)>,
}

impl<A: Allocator, I: Index<A>> Ycsb<A, I> {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            _index: PhantomData,
        }
    }
}

impl<A: Allocator, I: Index<A>> Deref for Ycsb<A, I> {
    type Target = Config;
    fn deref(&self) -> &Self::Target {
        &self.config
    }
}

pub struct Global<I> {
    index: I,
    acked: Shm<ycsb::Acknowledged>,
    tx: Mutex<Option<mpmc::Sender<Instant>>>,
    rx: mpmc::Receiver<Instant>,
}

unsafe impl<I> Sync for Global<I> {}

pub struct Coordinator {
    interval: Option<Duration>,
    sleeper: SpinSleeper,
    tx: Option<mpmc::Sender<Instant>>,
}

pub struct OutputThread {
    latency: Histogram<u64>,
    operation_count: u64,
}

#[derive(Serialize)]
pub struct Output {
    throughput: u64,
    latency_mean: u64,
    latency_p50: u64,
    latency_p90: u64,
    latency_p99: u64,
}

impl<B: Backend, I: Index<B::Allocator>> benchmark::Benchmark<B> for Ycsb<B::Allocator, I> {
    const NAME: &str = "/ycsb";
    type StateGlobal = Global<I>;
    type StateProcess = ();
    type StateCoordinator = Coordinator;
    type StateWorker = ();

    type OutputWorker = OutputThread;
    type OutputCoordinator = Duration;
    type OutputProcess = Output;

    fn setup_global(
        &self,
        config: &config::Process,
        allocator: &allocator::Config,
    ) -> Self::StateGlobal {
        let (tx, rx) = mpmc::unbounded();
        Global {
            index: I::new(
                Some(allocator.numa),
                "index",
                self.index.len,
                config.is_leader(),
                self.index.populate,
                config.thread_count,
            )
            .unwrap(),
            acked: Shm::builder()
                .name(c"/acked".to_owned())
                .create(config.is_leader())
                .populate(true)
                .build()
                .unwrap(),
            tx: Mutex::new(Some(tx)),
            rx,
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
        config: &config::Process,
        global: &Self::StateGlobal,
        (): &Self::StateProcess,
    ) -> Self::StateCoordinator {
        Coordinator {
            interval: self.throughput.map(|throughput| {
                Duration::from_nanos(10u64.pow(9) * (config.process_count as u64) / throughput)
            }),
            sleeper: SpinSleeper::default().with_spin_strategy(SpinStrategy::SpinLoopHint),
            tx: global.tx.lock().unwrap().take(),
        }
    }

    fn setup_worker(
        &self,
        config: &config::Thread,
        global: &Self::StateGlobal,
        (): &Self::StateProcess,
        allocator: &mut B::Allocator,
    ) -> Self::StateWorker {
        load(&self.workload, config, allocator, &global.index)
    }

    fn run_coordinator(
        &self,
        _config: &config::Process,
        _global: &Self::StateGlobal,
        (): &Self::StateProcess,
        coordinator: &mut Self::StateCoordinator,
    ) -> Self::OutputCoordinator {
        let tx = coordinator.tx.take().unwrap();
        let mut count = 0u64;
        let start = Instant::now();
        let time = Duration::from_nanos(self.time * 10u64.pow(9));

        match coordinator.interval {
            None => coordinator.sleeper.sleep(time),
            Some(interval) => {
                let mut ts = start;

                loop {
                    count += 1;
                    tx.send(ts).unwrap();

                    let next = ts + interval;
                    coordinator.sleeper.sleep_until(next);

                    if next.saturating_duration_since(start) >= time {
                        break;
                    } else {
                        ts = next;
                    }
                }

                let expected = (time.as_nanos() / interval.as_nanos()) as u64;
                assert!(
                    count.abs_diff(expected) * 100 / expected < 1,
                    "actual op count {count}, expected {expected}"
                );
            }
        }

        time
    }

    fn run_worker(
        &self,
        config: &config::Thread,
        global: &Self::StateGlobal,
        (): &Self::StateProcess,
        _worker: &mut Self::StateWorker,
        allocator: &mut B::Allocator,
    ) -> Self::OutputWorker {
        let mut runner = self
            .workload
            .runner(unsafe { global.acked.address().as_ref().unwrap() });

        self.run(config, &mut runner, allocator, global)
    }

    fn teardown_global(&self, config: &config::Process, mut global: Self::StateGlobal) {
        if !config.is_leader() {
            return;
        }

        global.index.unlink().unwrap();
        global.acked.unlink().unwrap();
    }

    fn aggregate(
        time: Self::OutputCoordinator,
        workers: Vec<Self::OutputWorker>,
    ) -> Self::OutputProcess {
        let latency = workers.iter().fold(
            Histogram::new(3).unwrap(),
            |mut acc,
             OutputThread {
                 latency,
                 operation_count: _,
             }| {
                acc.add(latency).unwrap();
                acc
            },
        );

        let operation_count = workers
            .iter()
            .map(
                |OutputThread {
                     latency: _,
                     operation_count,
                 }| *operation_count,
            )
            .sum::<u64>();

        Output {
            // FIXME: u128 division and/or casting bug?
            throughput: (operation_count as f64 / time.as_secs_f64()) as u64,
            latency_mean: latency.mean() as u64,
            latency_p50: latency.value_at_quantile(0.5),
            latency_p90: latency.value_at_quantile(0.9),
            latency_p99: latency.value_at_quantile(0.99),
        }
    }
}

impl Config {
    fn run<A: Allocator, I: Index<A>>(
        &self,
        config: &config::Thread,
        runner: &mut ycsb::Runner,
        allocator: &mut A,
        global: &Global<I>,
    ) -> OutputThread {
        let mut rng = SmallRng::seed_from_u64(config.thread_id as u64);
        let mut latency = Histogram::<u64>::new(3).unwrap();
        let mut operation_count = 0;

        loop {
            let ts = match self.throughput {
                None => match global.rx.try_recv() {
                    Err(TryRecvError::Disconnected) => break,
                    _ => Instant::now(),
                },
                Some(_) => match global.rx.recv() {
                    Ok(ts) => ts,
                    Err(RecvError) => break,
                },
            };

            let operation = runner.next_operation(&mut rng);
            match operation {
                ycsb::Operation::Read => {
                    let key = runner.next_key_read(&mut rng);
                    with(
                        config.thread_id,
                        allocator,
                        &global.index,
                        &key,
                        |value| unsafe {
                            let record = value.cast::<Record>().as_ref().unwrap();
                            for field in &record.0 {
                                (field as *const Field).read_volatile();
                            }
                        },
                    )
                }
                ycsb::Operation::Update => {
                    let key = runner.next_key_read(&mut rng);
                    let field = runner.next_field(&mut rng);
                    with(
                        config.thread_id,
                        allocator,
                        &global.index,
                        &key,
                        |value| unsafe {
                            let record = value.cast::<Record>().as_ref().unwrap();
                            record.0[field as usize].value[0].store(1, Ordering::Release);
                        },
                    );
                }
                ycsb::Operation::Scan => todo!(),
                ycsb::Operation::Insert => {
                    let key = runner.next_key_insert(&mut rng);
                    insert(config.thread_id, allocator, &global.index, &key);
                    runner.acknowledge(key);
                }
                ycsb::Operation::ReadModifyWrite => todo!(),
            }

            latency.record(ts.elapsed().as_nanos() as u64).unwrap();
            operation_count += 1;
        }

        OutputThread {
            latency,
            operation_count,
        }
    }
}

fn with<A: Allocator, I: Index<A>, F: FnOnce(*const u8)>(
    thread_id: usize,
    allocator: &mut A,
    index: &I,
    key: &ycsb::Key,
    with: F,
) {
    let found = index.get(thread_id, allocator, &key.id().to_ne_bytes(), with);
    assert!(found);
}
