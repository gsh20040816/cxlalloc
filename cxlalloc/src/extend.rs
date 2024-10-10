use crate::atomic::Packed;

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
#[cfg(feature = "extend")]
pub(crate) fn spawn(raw: &crate::raw::Heap) -> std::thread::JoinHandle<()> {
    let capacity = raw.capacity;
    let process_id = raw.process_id;

    // Maintain weak reference to allow main thread to run
    // destructor and deallocate memory.
    //
    // It's still possible for the main thread to finish without
    // blocking on deallocation if the extension thread holds a
    // strong reference, either while it's polling or in the
    // middle of an extension. But
    let weak = std::sync::Arc::downgrade(raw);

    std::thread::spawn(move || {
        loop {
            core::hint::spin_loop();

            let Some(raw) = weak.upgrade() else {
                return;
            };

            let heap = raw.heap();
            let barrier = heap.shared.barrier();

            if !barrier.has_request(process_id) {
                continue;
            }

            use crate::region::shared::Request;
            let epoch = match heap.shared.request() {
                None => unreachable!(),
                Some(Request::Map(_)) => todo!(),
                Some(Request::Extend(epoch)) => epoch,
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
            if cfg!(feature = "stat-extend") {
                let start = std::time::Instant::now();
                raw.extend().unwrap();
                let elapsed = start.elapsed().as_nanos();
                println!(
                    "{},kernel,{}",
                    epoch.total(capacity) as usize * crate::SIZE_SLAB,
                    elapsed
                );
            } else {
                raw.extend().unwrap();
            }

            barrier.acknowledge(process_id);
        }
    })
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Epoch(u8);

impl Epoch {
    /// The total size of all epochs up to and including this one in bytes.
    pub fn total_byte(&self, initial: usize) -> usize {
        2usize.pow(self.0 as u32) * initial
    }

    /// The total size of all epochs up to and including this one in slabs.
    pub(crate) fn total(&self, initial: u32) -> u32 {
        2u32.pow(self.0 as u32) * initial
    }

    /// The offset of this last epoch.
    pub(crate) fn offset(&self, initial: u32) -> u32 {
        match self.0 {
            0 => 0,
            _ => Epoch(self.0 - 1).total(initial),
        }
    }

    /// The size of this last epoch.
    pub(crate) fn partial(&self, initial: u32) -> u32 {
        match self.0 {
            0 => initial,
            _ => Epoch(self.0 - 1).total(initial),
        }
    }

    pub(crate) fn next(&self) -> Self {
        Self(self.0 + 1)
    }
}

unsafe impl Packed for Epoch {
    const BITS: u8 = 8;

    fn pack(&self) -> u64 {
        self.0 as u64
    }

    fn unpack(value: u64) -> Self {
        Self(value as u8)
    }
}

impl From<Epoch> for u8 {
    fn from(Epoch(epoch): Epoch) -> Self {
        epoch
    }
}
