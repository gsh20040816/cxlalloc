pub mod generator;

use core::hash::Hash as _;
use core::hash::Hasher as _;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::Ordering;

use generator::Generator as _;
use generator::number;
use rand::Rng;
use rapidhash::RapidHasher;
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
    request_distribution: number::Distribution,
}

pub struct Loader {
    insert_order: InsertOrder,
    key_sequence: AtomicU64,
}

pub struct Runner {
    operation_chooser: generator::Discrete<Operation>,
    key_chooser: generator::Number,
    field_count: usize,
    field_chooser: generator::Number,
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

        Runner {
            operation_chooser,
            field_count: self.field_count,
            key_chooser: match self.request_distribution {
                number::Distribution::Constant => unreachable!(),
                number::Distribution::Uniform => {
                    generator::Number::uniform(self.record_count as u64)
                }
                number::Distribution::Zipfian => {
                    generator::Number::zipfian(self.record_count as u64)
                }
            },
            field_chooser: generator::Number::uniform(self.field_count as u64),
        }
    }
}

impl Loader {
    #[inline]
    pub fn next_key(&mut self) -> u64 {
        let key = self.key_sequence.fetch_add(1, Ordering::Relaxed);
        match self.insert_order {
            InsertOrder::Ordered => key,
            InsertOrder::Hashed => {
                let mut hasher = RapidHasher::default();
                key.hash(&mut hasher);
                hasher.finish()
            }
        }
    }
}

impl Runner {
    #[inline]
    pub fn next_operation<R: Rng>(&mut self, rng: &mut R) -> Operation {
        self.operation_chooser.next(rng)
    }

    #[inline]
    pub fn field_count(&self) -> usize {
        self.field_count
    }

    #[inline]
    pub fn next_key<R: Rng>(&mut self, rng: &mut R) -> u64 {
        self.key_chooser.next(rng)
    }

    #[inline]
    pub fn next_field<R: Rng>(&mut self, rng: &mut R) -> u64 {
        self.field_chooser.next(rng)
    }

    // FIXME
    #[inline]
    pub fn next_field_length<R: Rng>(&mut self, _: &mut R) -> u64 {
        100
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
    use crate::generator::number;

    pub(super) fn insert_order() -> InsertOrder { InsertOrder::Hashed }
    pub(super) fn field_count() -> usize { 10 }
    pub(super) fn read_all_fields() -> bool { true}
    pub(super) fn read_proportion() -> f32 { 0.95 }
    pub(super) fn update_proportion() -> f32 { 0.05 }
    pub(super) fn request_distribution() -> number::Distribution { number::Distribution::Zipfian }
}

#[derive(Copy, Clone, Debug, Deserialize)]
pub enum InsertOrder {
    #[serde(alias = "ordered")]
    Ordered,

    #[serde(alias = "hashed")]
    Hashed,
}
