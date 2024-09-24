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
    process_id: usize,
}

const BARRIER: root::Index = root::Index::new(0);

fn main() {
    env_logger::init();

    let command = Command::parse();
    let destroy = command.process_id == 0;

    for _ in 0..command.iterations {
        let backend = cxlalloc::raw::Backend::Shm(backend::Shm::new(destroy));

        let raw = cxlalloc::raw::Builder::default()
            .backend(backend)
            .thread_count(command.process_count)
            .process_id(command.process_id)
            .process_count(command.process_count)
            .size(command.size)
            .build("ex")
            .unwrap();

        let mut allocator =
            raw.allocator(unsafe { cxlalloc::thread::Id::new(command.process_id as u16) });

        if command.process_id == 0 {
            // Ad hoc barrier
            let allocation = unsafe { allocator.allocate_untyped(8) };
            let barrier = unsafe { allocation.cast::<AtomicU64>().as_ref().unwrap() };

            barrier.store(1, Ordering::Release);
            unsafe { allocator.set_root_untyped(BARRIER, NonNull::new(allocation)) };
            while barrier.load(Ordering::Acquire) < command.process_count as u64 {
                std::hint::spin_loop();
            }

            for epoch in 0..command.count {
                let start = Instant::now();
                allocator.extend();
                let time = start.elapsed().as_nanos();
                println!("{},total,{}", command.size * 2usize.pow(epoch as u32), time);
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

            while allocator.epoch() < command.count {
                std::thread::sleep(Duration::from_secs(1));
            }
        }
    }
}
