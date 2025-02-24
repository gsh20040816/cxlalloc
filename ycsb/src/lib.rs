pub mod generator;

use core::hash::Hash as _;
use core::hash::Hasher as _;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::AtomicUsize;
use core::sync::atomic::Ordering;

use rand::Rng;
use rapidhash::RAPID_SEED;
use rapidhash::RapidHasher;
use rapidhash::rapidhash;
use serde::Deserialize;

pub trait Database {}

#[derive(Debug, Deserialize)]
pub struct Workload {
    #[serde(alias = "insertstart", default)]
    insert_start: usize,

    #[serde(alias = "insertcount", default)]
    insert_count: Option<usize>,

    #[serde(alias = "insertorder", default = "default::insert_order")]
    insert_order: InsertOrder,

    #[serde(alias = "fieldcount", default = "default::field_count")]
    field_count: usize,

    #[serde(alias = "recordcount")]
    record_count: usize,

    #[serde(alias = "operationcount")]
    operation_count: usize,

    #[serde(alias = "readallfields", default = "default::read_all_fields")]
    read_all_fields: bool,

    #[serde(alias = "readproportion", default = "default::read_proportion")]
    read_proportion: f32,

    #[serde(alias = "updateproportion", default = "default::update_proportion")]
    update_proportion: f32,

    #[serde(alias = "scanproportion", default)]
    scan_proportion: f32,

    #[serde(alias = "insertproportion", default)]
    insert_proportion: f32,

    #[serde(
        alias = "requestdistribution",
        default = "default::request_distribution"
    )]
    request_distribution: RequestDistribution,
}

pub struct Loader {
    insert_order: InsertOrder,
    key_sequence: AtomicU64,
}

pub struct Runner {
    operation_chooser: generator::Discrete<Operation>,
}

impl Workload {
    pub fn loader(&self) -> Loader {
        Loader {
            insert_order: self.insert_order,
            key_sequence: AtomicU64::new(self.insert_start as u64),
        }
    }

    pub fn runner(&self) -> Runner {
        let operation_chooser = generator::Discrete::new(vec![
            (Operation::Read, self.read_proportion),
            (Operation::Update, self.update_proportion),
            (Operation::Scan, self.scan_proportion),
            (Operation::Insert, self.insert_proportion),
        ]);

        Runner { operation_chooser }
    }
}

impl Loader {
    pub fn next_key(&mut self) -> u64 {
        let key = self.key_sequence.fetch_add(1, Ordering::Relaxed);
        match self.insert_order {
            InsertOrder::Ordered => key,
            InsertOrder::Hashed => {
                let mut hasher = RapidHasher::new(RAPID_SEED);
                key.hash(&mut hasher);
                hasher.finish()
            }
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub enum Operation {
    Read,
    Update,
    Scan,
    Insert,
}

#[rustfmt::skip]
mod default {
    use crate::InsertOrder;
    use super::RequestDistribution;

    pub(super) fn insert_order() -> InsertOrder { InsertOrder::Hashed }
    pub(super) fn field_count() -> usize { 10 }
    pub(super) fn read_all_fields() -> bool { true}
    pub(super) fn read_proportion() -> f32 { 0.95 }
    pub(super) fn update_proportion() -> f32 { 0.05 }
    pub(super) fn request_distribution() -> RequestDistribution { RequestDistribution::Zipfian }
}

#[derive(Copy, Clone, Debug, Deserialize)]
pub enum InsertOrder {
    #[serde(alias = "ordered")]
    Ordered,

    #[serde(alias = "hashed")]
    Hashed,
}

#[derive(Debug, Deserialize)]
pub enum FieldLengthDistribution {
    #[serde(alias = "constant")]
    Constant,
}

#[derive(Debug, Deserialize)]
pub enum RequestDistribution {
    #[serde(alias = "uniform")]
    Uniform,

    #[serde(alias = "zipfian")]
    Zipfian,
}
