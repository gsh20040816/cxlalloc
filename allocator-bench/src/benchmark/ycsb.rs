use core::mem;
use core::sync::atomic::AtomicU8;
use core::sync::atomic::Ordering;
use core::time::Duration;
use std::sync::Mutex;
use std::time::Instant;

use bon::Builder;
use crossbeam_channel as mpmc;
use hdrhistogram::Histogram;
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

#[derive(Builder, Clone, Debug, Deserialize, Serialize)]
pub struct Ycsb {
    /// Whether to measure loading only (or else running phase only)
    load: bool,

    index: index::Config,

    throughput: u64,

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

struct Record([Field; 10]);

pub struct OutputThread {
    latency: Histogram<u64>,
}

#[derive(Serialize)]
pub struct Output {
    latency_mean: u64,
    latency_p50: u64,
    latency_p90: u64,
    latency_p99: u64,
}

#[repr(C)]
struct Field {
    value: [AtomicU8; 96],
}

pub struct Coordinator {
    interval: Duration,
    sleeper: SpinSleeper,
    tx: Option<mpmc::Sender<Instant>>,
}

unsafe impl<I> Sync for Global<I> {}

impl<B: Backend, I: Index<B::Allocator>> benchmark::Benchmark<B, I> for Ycsb {
    const NAME: &str = "ycsb";
    type Global = Global<I>;

    type Coordinator = Coordinator;
    type Worker = ();

    type Thread = OutputThread;
    type Process = ();
    type Output = Output;

    fn setup_process(
        &self,
        _config: &config::Process,
        allocator: &allocator::Config,
    ) -> Self::Global {
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
        global: &Self::Global,
    ) -> Self::Coordinator {
        Coordinator {
            interval: Duration::from_nanos(
                10u64.pow(9) * (config.process_count as u64) / self.throughput,
            ),
            sleeper: SpinSleeper::default().with_spin_strategy(SpinStrategy::SpinLoopHint),
            tx: global.tx.lock().unwrap().take(),
        }
    }

    fn setup_worker(
        &self,
        config: &config::Thread,
        global: &Self::Global,
        allocator: &mut B::Allocator,
    ) -> Self::Worker {
        if self.load {
            return;
        }

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
        _global: &Self::Global,
        coordinator: &mut Self::Coordinator,
    ) -> Self::Process {
        if self.load {
            todo!()
        }

        let tx = coordinator.tx.take().unwrap();
        let mut count = 0u64;
        let start = Instant::now();
        let mut ts = start;

        loop {
            count += 1;
            tx.send(ts).unwrap();

            let next = ts + coordinator.interval;
            coordinator.sleeper.sleep_until(next);

            if next.saturating_duration_since(start).as_secs() > self.time {
                break;
            } else {
                ts = next;
            }
        }

        let expected = self.time * self.throughput;
        assert!(
            count.abs_diff(expected) * 100 / expected < 1,
            "actual op count {count}, expected {expected}"
        );
    }

    fn run_worker(
        &self,
        config: &config::Thread,
        global: &Self::Global,
        _local: &mut Self::Worker,
        allocator: &mut B::Allocator,
    ) -> Self::Thread {
        if self.load {
            match self.index.inline {
                true => {
                    load::<true, _, _>(self.write, &self.workload, config, allocator, &global.index)
                }
                false => load::<false, _, _>(
                    self.write,
                    &self.workload,
                    config,
                    allocator,
                    &global.index,
                ),
            }

            todo!()
        }

        let mut runner = self
            .workload
            .runner(unsafe { global.acked.address().as_ref().unwrap() });

        match self.index.inline {
            true => run::<true, _, _>(self.write, &mut runner, allocator, global),
            false => run::<false, _, _>(self.write, &mut runner, allocator, global),
        }
    }

    fn teardown_process(&self, config: &config::Process, mut global: Self::Global) {
        if config.process_id != 0 {
            return;
        }

        global.index.unlink().unwrap();
        global.acked.unlink().unwrap();
    }

    fn aggregate((): Self::Process, threads: Vec<Self::Thread>) -> Self::Output {
        let latency = threads.into_iter().fold(
            Histogram::new(3).unwrap(),
            |mut acc, OutputThread { latency }| {
                acc.add(latency).unwrap();
                acc
            },
        );

        Output {
            latency_mean: latency.mean() as u64,
            latency_p50: latency.value_at_quantile(0.5),
            latency_p90: latency.value_at_quantile(0.9),
            latency_p99: latency.value_at_quantile(0.99),
        }
    }
}

fn load<const INLINE: bool, A: Allocator, I: Index<A>>(
    write: bool,
    workload: &ycsb::Workload,
    config: &config::Thread,
    allocator: &mut A,
    index: &I,
) {
    let mut loader = workload.loader(config.thread_total(), config.thread_id);

    while let Some(key) = loader.next_key() {
        insert::<INLINE, _, _>(write, allocator, index, &key);
    }
}

fn run<const INLINE: bool, A: Allocator, I: Index<A>>(
    write: bool,
    runner: &mut ycsb::Runner,
    allocator: &mut A,
    global: &Global<I>,
) -> OutputThread {
    let mut rng = rand::rng();
    let mut latency = Histogram::<u64>::new(3).unwrap();

    while let Ok(ts) = global.rx.recv() {
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
                insert::<INLINE, _, _>(write, allocator, &global.index, &key);
            }
            ycsb::Operation::ReadModifyWrite => todo!(),
        }

        latency.record(ts.elapsed().as_nanos() as u64).unwrap();
    }

    OutputThread { latency }
}

fn insert<const INLINE: bool, A: Allocator, I: Index<A>>(
    write: bool,
    allocator: &mut A,
    index: &I,
    key: &ycsb::Key,
) {
    const SIZE: usize = mem::size_of::<Record>();
    match INLINE {
        true => index.insert(allocator, key.id(), SIZE, |_, pointer| {
            if write {
                unsafe {
                    libc::memset(pointer.cast(), 0xff, SIZE);
                }
            }
        }),
        false => {
            let value = allocator.allocate(SIZE).unwrap();

            if write {
                unsafe {
                    libc::memset(value.as_ptr(), 0xff, SIZE);
                }
            }

            index.insert(
                allocator,
                key.id(),
                mem::size_of::<u64>(),
                |allocator, pointer| unsafe {
                    allocator.link(pointer.cast(), &value);
                },
            );
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
