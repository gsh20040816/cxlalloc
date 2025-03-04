use std::sync::LazyLock;

use crate::RequestDistribution;
use crate::Workload;

pub static A: LazyLock<Workload> = LazyLock::new(|| {
    Workload::builder()
        .read_proportion(0.5)
        .update_proportion(0.5)
        .build()
});

pub static B: LazyLock<Workload> = LazyLock::new(|| {
    Workload::builder()
        .read_proportion(0.95)
        .update_proportion(0.05)
        .build()
});

pub static C: LazyLock<Workload> = LazyLock::new(|| {
    Workload::builder()
        .read_proportion(1.0)
        .update_proportion(0.0)
        .build()
});

pub static D: LazyLock<Workload> = LazyLock::new(|| {
    Workload::builder()
        .read_proportion(0.95)
        .update_proportion(0.0)
        .insert_proportion(0.05)
        .request_distribution(RequestDistribution::Latest)
        .build()
});
