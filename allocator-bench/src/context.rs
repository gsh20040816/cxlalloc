use core::ops::Deref;

use clap::Parser;
use serde::Deserialize;
use serde::Serialize;

#[derive(Copy, Clone, Parser, Deserialize, Serialize)]
pub struct Global {
    /// NUMA node for remote memory
    #[arg(long)]
    pub numa: usize,

    /// Eagerly populate page tables
    #[arg(long)]
    pub populate: bool,

    /// Number of processes
    #[arg(long)]
    pub process_count: usize,

    /// Number of threads per process
    #[arg(long)]
    pub thread_count: usize,
}

impl Global {
    pub fn thread_total(&self) -> usize {
        self.process_count * self.thread_count
    }
}

#[derive(Copy, Clone, Parser, Deserialize, Serialize)]
pub struct Process {
    #[serde(flatten)]
    #[command(flatten)]
    pub global: Global,

    /// Unique process ID within range 0..process_count
    #[arg(long)]
    pub process_id: usize,
}

impl Deref for Process {
    type Target = Global;
    fn deref(&self) -> &Self::Target {
        &self.global
    }
}

#[derive(Copy, Clone)]
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
