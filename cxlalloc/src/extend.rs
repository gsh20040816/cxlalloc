use std::sync::Arc;
use std::thread;

use crate::raw;
use crate::region::shared;

/// This thread is responsible for heap extension.
///
/// We use a dedicated thread per process to simplify when
/// several threads (in the same process) concurrently try to
/// extend the heap. This way, we don't need to coordinate
/// which competing thread calls `mmap`, which should only
/// be called once per process.
///
/// This thread polls for `metadata::Operation::Capacity { .. }`,
/// then initiates and/or participates in a barrier after calling
/// `mmap` for this process. The thread(s) that originally submitted
/// the heap extension operation are unblocked after the barrier
/// completes, i.e. after all processes have called `mmap`.
///
/// One benefit of this approach is that no threads are blocked
/// by heap extension until they specifically request additional
/// backing memory by modifying `metadata.extent`.
pub(crate) fn spawn(raw: &raw::Heap) -> thread::JoinHandle<()> {
    let process_id = raw.process_id;

    // Maintain weak reference to allow main thread to run
    // destructor and deallocate memory.
    //
    // It's still possible for the main thread to finish without
    // blocking on deallocation if the extension thread holds a
    // strong reference, either while it's polling or in the
    // middle of an extension. But
    let weak = Arc::downgrade(raw);

    std::thread::spawn(move || {
        loop {
            sleep();

            let Some(raw) = weak.upgrade() else {
                return;
            };

            let heap = raw.heap();
            let barrier = heap.shared.barrier();

            if !barrier.has_request(process_id) {
                continue;
            }

            match heap.shared.request() {
                None => unreachable!(),
                Some(shared::Request::Map(_)) => todo!(),
                Some(shared::Request::Extend(epoch)) => epoch,
            };

            // Execute `mmap` if we haven't already
            //
            // NOTE: I believe there is an (extremely unlikely) race condition here.
            // Consider the following execution of two processes P0, P1:
            //
            // 1) P0 initiates heap extension
            // 2) P1 calls mmap and updates barrier
            // 3) P1 crashes
            // 4) P0 calls mmap and updates barrier
            // 5) P1 respawns and does not mmap again (epoch is still 0)
            // 6) P0 updates epoch to 1
            //
            // Seems quite unlikely since process respawn is
            // relatively slow. And if a process crashes before
            // updating the barrier, the other processes will block.
            //
            // There is inevitably a delay between reading a finished
            // barrier and updating the epoch, unless we pack the
            // barrier and epoch into the same memory location. But the
            // claim metadata already occupies 48 bits.
            raw.extend().unwrap();
            barrier.acknowledge(process_id);
        }
    })
}

#[inline]
pub(crate) fn sleep() {
    core::hint::spin_loop()
}
