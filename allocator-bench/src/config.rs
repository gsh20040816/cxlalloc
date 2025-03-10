use core::ops::Deref;

use bon::Builder;
use serde::Deserialize;
use serde::Serialize;

#[derive(Builder, Copy, Clone, Debug, Deserialize, Serialize)]
pub struct Global {
    /// Number of processes
    pub process_count: usize,

    /// Number of threads per process
    pub thread_count: usize,
}

impl Global {
    pub fn thread_total(&self) -> usize {
        self.process_count * self.thread_count
    }

    pub fn with_process_id(&self, process_id: usize) -> Process {
        Process {
            global: *self,
            process_id,
        }
    }
}

#[derive(Copy, Clone, Debug, Deserialize, Serialize)]
pub struct Process {
    #[serde(flatten)]
    pub global: Global,

    /// Unique process ID within range 0..process_count
    pub process_id: usize,
}

impl Deref for Process {
    type Target = Global;
    fn deref(&self) -> &Self::Target {
        &self.global
    }
}

#[derive(Copy, Clone, Debug)]
pub struct Thread {
    pub process: Process,

    /// Unique thread ID within range 0..process_count * thread_count
    pub thread_id: usize,
}

impl Deref for Thread {
    type Target = Process;
    fn deref(&self) -> &Self::Target {
        &self.process
    }
}
