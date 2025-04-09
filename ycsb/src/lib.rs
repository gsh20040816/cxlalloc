pub mod generator;

use core::hash::Hash as _;
use core::hash::Hasher as _;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::Ordering;

use bon::Builder;
use generator::Generator as _;
use rand::Rng;
use rapidhash::RapidHasher;
use serde::Deserialize;
use serde::Serialize;

pub mod workload;

#[derive(Builder, Clone, Debug, Deserialize, Serialize)]
#[builder(state_mod(vis = "pub"), derive(Clone, Debug))]
pub struct Workload {
    #[builder(default = default::insert_order())]
    #[serde(alias = "insertorder", default = "default::insert_order")]
    pub insert_order: InsertOrder,

    #[builder(default = default::field_count())]
    #[serde(alias = "fieldcount", default = "default::field_count")]
    pub field_count: usize,

    #[builder(default = default::record_count())]
    #[serde(alias = "recordcount", default = "default::record_count")]
    pub record_count: usize,

    #[builder(default = default::operation_count())]
    #[serde(alias = "operationcount", default = "default::operation_count")]
    pub operation_count: usize,

    #[builder(default = default::read_all_fields())]
    #[serde(alias = "readallfields", default = "default::read_all_fields")]
    pub read_all_fields: bool,

    #[builder(default = default::read_proportion())]
    #[serde(alias = "readproportion", default = "default::read_proportion")]
    pub read_proportion: f32,

    #[builder(default = default::update_proportion())]
    #[serde(alias = "updateproportion", default = "default::update_proportion")]
    pub update_proportion: f32,

    #[builder(default)]
    #[serde(alias = "scanproportion", default)]
    pub scan_proportion: f32,

    #[builder(default)]
    #[serde(alias = "insertproportion", default)]
    pub insert_proportion: f32,

    #[builder(default)]
    #[serde(alias = "readmodifywriteproportion", default)]
    pub read_modify_write_proportion: f32,

    #[builder(default)]
    #[serde(alias = "deleteproportion", default)]
    pub delete_proportion: f32,

    #[builder(default = default::request_distribution())]
    #[serde(
        alias = "requestdistribution",
        default = "default::request_distribution"
    )]
    pub request_distribution: RequestDistribution,
}

pub struct Loader {
    insert_order: InsertOrder,
    next_key: u64,
    last_key: u64,
}

pub struct Runner<'a> {
    acked: &'a Acknowledged,
    record_count: usize,
    operation_chooser: generator::Discrete<Operation>,
    insert_order: InsertOrder,
    request_distribution: RequestDistribution,
    keys_total: u64,
    key_chooser: generator::Number,
    field_count: usize,
    field_chooser: generator::Number,
}

impl Workload {
    pub fn operation_count(&self) -> usize {
        self.operation_count
    }

    pub fn field_count(&self) -> usize {
        self.field_count
    }

    pub fn record_count(&self) -> usize {
        self.record_count
    }

    pub fn loader(&self, thread_count: usize, thread_id: usize) -> Loader {
        assert_eq!(
            self.record_count % thread_count,
            0,
            "Record count {} must be divisible by thread total {}",
            self.record_count,
            thread_count
        );

        let insert_count = (self.record_count / thread_count) as u64;
        let insert_start = insert_count * thread_id as u64;
        Loader {
            insert_order: self.insert_order,
            next_key: insert_start,
            last_key: insert_start + insert_count,
        }
    }

    pub fn runner<'a>(&self, acked: &'a Acknowledged) -> Runner<'a> {
        let operation_chooser = generator::Discrete::new(vec![
            (Operation::Read, self.read_proportion),
            (Operation::Update, self.update_proportion),
            (Operation::Scan, self.scan_proportion),
            (Operation::Insert, self.insert_proportion),
            (
                Operation::ReadModifyWrite,
                self.read_modify_write_proportion,
            ),
            (Operation::Delete, self.delete_proportion),
        ]);

        let key_count_new = self.insert_proportion * (self.operation_count as f32) * 2.0;
        let key_count_total = self.record_count as u64 + key_count_new as u64;

        Runner {
            acked,
            record_count: self.record_count,
            operation_chooser,
            field_count: self.field_count,
            insert_order: self.insert_order,
            request_distribution: self.request_distribution,
            keys_total: key_count_total,
            key_chooser: match self.request_distribution {
                RequestDistribution::Latest => generator::Number::zipfian(key_count_total),
                RequestDistribution::Uniform => generator::Number::uniform(key_count_total),
                RequestDistribution::Zipfian => generator::Number::zipfian(key_count_total),
            },
            field_chooser: generator::Number::uniform(self.field_count as u64),
        }
    }
}

impl Loader {
    #[inline]
    pub fn next_key(&mut self) -> Option<Key> {
        if self.next_key >= self.last_key {
            return None;
        }

        let key = self.next_key;
        self.next_key += 1;
        Some(Key::new(self.insert_order, key))
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Key(u64);

impl Key {
    const HASHED: u64 = 1 << 63;

    #[inline]
    fn new(order: InsertOrder, sequence: u64) -> Self {
        match order {
            InsertOrder::Ordered => Self(sequence),
            InsertOrder::Hashed => Self(sequence | Self::HASHED),
        }
    }

    #[inline]
    fn sequence(&self) -> u64 {
        self.0 & !Self::HASHED
    }

    #[inline]
    pub fn id(&self) -> u64 {
        match self.0 & Self::HASHED > 0 {
            false => self.sequence(),
            true => {
                let mut hasher = RapidHasher::default();
                self.sequence().hash(&mut hasher);
                hasher.finish()
            }
        }
    }
}

impl Runner<'_> {
    #[inline]
    pub fn next_operation<R: Rng>(&mut self, rng: &mut R) -> Operation {
        self.operation_chooser.next(rng)
    }

    #[inline]
    pub fn field_count(&self) -> usize {
        self.field_count
    }

    #[inline]
    pub fn next_key_insert<R: Rng>(&mut self, rng: &mut R) -> Key {
        self.next_key_inner(rng, 1)
    }

    #[inline]
    pub fn next_key_read<R: Rng>(&mut self, rng: &mut R) -> Key {
        self.next_key_inner(rng, 0)
    }

    #[inline]
    fn next_key_inner<R: Rng>(&mut self, rng: &mut R, window: u64) -> Key {
        let max = self.record_count as u64 - 1 + self.acked.max() + window;
        let key = loop {
            let key = match self.request_distribution {
                RequestDistribution::Uniform => self.key_chooser.next(rng),
                RequestDistribution::Latest => match max.checked_sub(self.key_chooser.next(rng)) {
                    Some(key) => break key,
                    None => continue,
                },
                RequestDistribution::Zipfian => {
                    let key = self.key_chooser.next(rng);
                    let mut hasher = RapidHasher::default();
                    key.hash(&mut hasher);
                    hasher.finish() % self.keys_total
                }
            };

            if key <= max {
                break key;
            }
        };

        Key::new(self.insert_order, key)
    }

    #[inline]
    pub fn next_field<R: Rng>(&mut self, rng: &mut R) -> u64 {
        self.field_chooser.next(rng)
    }

    /// Only track newly inserted keys
    #[inline]
    pub fn acknowledge(&self, key: Key) {
        let Some(index) = key.sequence().checked_sub(self.record_count as u64) else {
            return;
        };
        self.acked.acknowledge(index);
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
    ReadModifyWrite,
    Delete,
}

#[rustfmt::skip]
mod default {
    use crate::InsertOrder;
    use crate::RequestDistribution;

    pub(super) fn insert_order() -> InsertOrder { InsertOrder::Hashed }
    pub(super) fn record_count() -> usize { 1_000 }
    pub(super) fn operation_count() -> usize { 1_000 }
    pub(super) fn field_count() -> usize { 10 }
    pub(super) fn read_all_fields() -> bool { true }
    pub(super) fn read_proportion() -> f32 { 0.95 }
    pub(super) fn update_proportion() -> f32 { 0.05 }
    pub(super) fn request_distribution() -> RequestDistribution { RequestDistribution::Zipfian }
}

#[derive(Copy, Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestDistribution {
    Latest,
    Uniform,
    Zipfian,
}

#[derive(Copy, Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InsertOrder {
    Ordered,
    Hashed,
}

#[repr(C)]
pub struct Acknowledged {
    hint: AtomicU64,

    inner: [AtomicU64; 1 << 20],
}

impl Default for Acknowledged {
    fn default() -> Self {
        Self::new()
    }
}

impl Acknowledged {
    pub fn new() -> Self {
        Self {
            hint: AtomicU64::new(0),
            inner: std::array::from_fn(|_| AtomicU64::new(0)),
        }
    }

    /// Max index (non-inclusive) such that all previous indices have been acknowledged.
    fn max(&self) -> u64 {
        let (i, j) = self.next();
        i * 64 + j
    }

    fn acknowledge(&self, index: u64) {
        let i = index / 64;
        let j = index % 64;

        self.inner[i as usize].fetch_or(1 << j, Ordering::Relaxed);
        let (hint, _) = self.next();
        self.hint.fetch_max(hint, Ordering::Relaxed);
    }

    fn next(&self) -> (u64, u64) {
        self.inner
            .iter()
            .enumerate()
            .skip(self.hint.load(Ordering::Relaxed) as usize)
            .find_map(
                |(i, row)| match row.load(Ordering::Relaxed).trailing_ones() {
                    64 => None,
                    j => Some((i as u64, j as u64)),
                },
            )
            .expect("Full acknowledgement array")
    }
}
