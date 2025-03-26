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
use crate::allocator::Handle as _;
use crate::benchmark;
use crate::config;
use crate::index;

use super::ycsb_load::Field;
use super::ycsb_load::Record;
use super::ycsb_load::insert;
use super::ycsb_load::load;

#[derive(Builder, Clone, Debug, Deserialize, Serialize)]
pub struct Ycsb {
    index: index::Config,

    throughput: Option<u64>,

    time: u64,

    /// Whether to write value
    write: bool,

    #[serde(flatten)]
    workload: ycsb::Workload,
}

// HACK: CXL-SHM doesn't support allocations larger than 1KiB (1_000B data + 24B header)
#[expect(unused)]
const MAX_SIZE: usize = 1_000;

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

impl<B: Backend, I: Index<B::Allocator>> benchmark::Benchmark<B, I> for Ycsb {
    const NAME: &str = "ycsb";
    type StateGlobal = Global<I>;

    type StateCoordinator = Coordinator;
    type StateWorker = ();

    type OutputWorker = OutputThread;
    type OutputCoordinator = Duration;
    type OutputGlobal = Output;

    fn setup_process(
        &self,
        _config: &config::Process,
        allocator: &allocator::Config,
    ) -> Self::StateGlobal {
        let (tx, rx) = mpmc::unbounded();
        Global {
            index: I::new(
                Some(allocator.numa),
                "index",
                self.index.len,
                self.index.populate,
            )
            .unwrap(),
            acked: Shm::new(None, c"acked".to_owned(), true).unwrap(),
            tx: Mutex::new(Some(tx)),
            rx,
        }
    }

    fn setup_coordinator(
        &self,
        config: &config::Process,
        global: &Self::StateGlobal,
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
        allocator: &mut B::Allocator,
    ) -> Self::StateWorker {
        match self.index.inline {
            true => {
                load::<true, _, _>(self.write, &self.workload, config, allocator, &global.index)
            }
            false => {
                load::<false, _, _>(self.write, &self.workload, config, allocator, &global.index)
            }
        }
    }

    fn run_coordinator(
        &self,
        _config: &config::Process,
        _global: &Self::StateGlobal,
        coordinator: &mut Self::StateCoordinator,
    ) -> Self::OutputCoordinator {
        let tx = coordinator.tx.take().unwrap();
        let mut count = 0u64;
        let start = Instant::now();
        let time = Duration::from_nanos(self.time);

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

                let expected = self.time / interval.as_nanos() as u64;
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
        _worker: &mut Self::StateWorker,
        allocator: &mut B::Allocator,
    ) -> Self::OutputWorker {
        let mut runner = self
            .workload
            .runner(unsafe { global.acked.address().as_ref().unwrap() });

        match self.index.inline {
            true => self.run::<true, _, _>(config, &mut runner, allocator, global),
            false => self.run::<false, _, _>(config, &mut runner, allocator, global),
        }
    }

    fn teardown_process(&self, config: &config::Process, mut global: Self::StateGlobal) {
        if config.process_id != 0 {
            return;
        }

        global.index.unlink().unwrap();
        global.acked.unlink().unwrap();
    }

    fn aggregate(
        time: Self::OutputCoordinator,
        workers: Vec<Self::OutputWorker>,
    ) -> Self::OutputGlobal {
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
            throughput: (((operation_count as u128 * 10u128.pow(9)) / time.as_nanos())
                / 10u128.pow(9)) as u64,
            latency_mean: latency.mean() as u64,
            latency_p50: latency.value_at_quantile(0.5),
            latency_p90: latency.value_at_quantile(0.9),
            latency_p99: latency.value_at_quantile(0.99),
        }
    }
}

impl Ycsb {
    fn run<const INLINE: bool, A: Allocator, I: Index<A>>(
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

            let key = runner.next_key(&mut rng);
            let operation = runner.next_operation(&mut rng);
            match operation {
                ycsb::Operation::Read => {
                    with::<INLINE, _, _, _>(allocator, &global.index, &key, |value| unsafe {
                        let record = value.cast::<Record>().as_ref().unwrap();
                        for field in &record.0 {
                            (field as *const Field).read_volatile();
                        }
                    })
                }
                ycsb::Operation::Update => {
                    let field = runner.next_field(&mut rng);
                    with::<INLINE, _, _, _>(allocator, &global.index, &key, |value| unsafe {
                        let record = value.cast::<Record>().as_ref().unwrap();
                        record.0[field as usize].value[0].store(1, Ordering::Release);
                    });
                }
                ycsb::Operation::Scan => todo!(),
                ycsb::Operation::Insert => {
                    insert::<INLINE, _, _>(self.write, allocator, &global.index, &key);
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

fn with<const INLINE: bool, A: Allocator, I: Index<A>, F: FnOnce(*const u8)>(
    allocator: &mut A,
    index: &I,
    key: &ycsb::Key,
    with: F,
) {
    match INLINE {
        true => {
            let found = index.get(allocator, key.id(), |_, value| with(value));
            assert!(found);
        }
        false => {
            let found = index.get(allocator, key.id(), |allocator, pointer| {
                let offset = unsafe { pointer.cast::<u64>().read() };
                let handle = allocator.offset_to_handle(offset).unwrap();
                with(handle.as_ptr().cast())
            });
            assert!(found);
        }
    }
}
