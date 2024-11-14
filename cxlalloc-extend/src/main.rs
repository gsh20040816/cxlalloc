use std::ptr::NonNull;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::Instant;

use clap::Parser;
use cxlalloc::raw::backend;
use cxlalloc::root;

#[derive(Parser)]
struct Command {
    #[arg(short, long)]
    size: usize,

    #[arg(short, long)]
    count: u8,

    #[arg(short, long)]
    iterations: usize,

    #[arg(long)]
    process_count: usize,

    #[arg(long)]
    process_id: Option<usize>,
}

const BARRIER: root::Index = root::Index::new(0);
const ID: &str = "ex";

fn main() {
    env_logger::init();

    let command = Command::parse();

    match command.process_id {
        None => {
            let path = std::env::current_exe().unwrap();
            let handles = (0..command.process_count)
                .map(|process_id| {
                    std::process::Command::new(&path)
                        .arg("--size")
                        .arg(command.size.to_string())
                        .arg("--count")
                        .arg(command.count.to_string())
                        .arg("--iterations")
                        .arg(command.iterations.to_string())
                        .arg("--process-count")
                        .arg(command.process_count.to_string())
                        .arg("--process-id")
                        .arg(process_id.to_string())
                        .spawn()
                })
                .collect::<Result<Vec<_>, _>>()
                .unwrap();

            for mut handle in handles {
                handle.wait().unwrap();
            }
        }
        Some(process_id) => {
            for _ in 0..command.iterations {
                let backend = cxlalloc::raw::Backend::Shm(backend::Shm);

                let raw = cxlalloc::raw::Builder::default()
                    .backend(backend)
                    .thread_count(command.process_count)
                    .process_id(process_id)
                    .process_count(command.process_count)
                    .size(command.size)
                    .build(ID)
                    .unwrap();

                let mut allocator =
                    raw.allocator(unsafe { cxlalloc::thread::Id::new(process_id as u16) });

                if process_id == 0 {
                    // Ad hoc barrier
                    let allocation = unsafe { allocator.allocate_untyped(8) };
                    let barrier = unsafe { allocation.cast::<AtomicU64>().as_ref().unwrap() };

                    barrier.store(1, Ordering::Release);
                    unsafe { allocator.set_root_untyped(BARRIER, NonNull::new(allocation)) };
                    while barrier.load(Ordering::Acquire) < command.process_count as u64 {
                        std::hint::spin_loop();
                    }

                    for _ in 0..command.count {
                        let start = Instant::now();
                        let epoch = allocator.epoch();
                        allocator.extend();
                        let time = start.elapsed().as_nanos();
                        // Note: must be multiple of 32KiB to be accurate here
                        eprintln!("{},total,{}", epoch.total_byte(command.size), time);
                    }
                } else {
                    let barrier = loop {
                        match unsafe { allocator.root_untyped(BARRIER) } {
                            None => std::hint::spin_loop(),
                            Some(block) => break unsafe { block.cast::<AtomicU64>().as_ref() },
                        }
                    };

                    barrier.fetch_add(1, Ordering::AcqRel);
                    while barrier.load(Ordering::Acquire) < command.process_count as u64 {
                        std::hint::spin_loop();
                    }

                    while u8::from(allocator.epoch()) < command.count {
                        std::thread::sleep(Duration::from_secs(1));
                    }
                }
            }
        }
    }
}
