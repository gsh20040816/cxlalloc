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

#[derive(Debug, Deserialize)]
pub struct Workload {
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

    #[serde(alias = "readmodifywriteproportion", default)]
    read_modify_write_proportion: f32,

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

pub struct Runner<'a> {
    acked: &'a Acknowledged,
    record_count: usize,
    operation_chooser: generator::Discrete<Operation>,
    insert_order: InsertOrder,
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

    pub fn loader(&self, thread_total: usize, thread_id: usize) -> Loader {
        Loader {
            insert_order: self.insert_order,
            key_sequence: AtomicU64::new((self.record_count / thread_total * thread_id) as u64),
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
        ]);

        let keys_new = self.insert_proportion * (self.operation_count as f32) * 2.0;
        let keys_total = self.record_count as u64 + keys_new as u64;

        Runner {
            acked,
            record_count: self.record_count,
            operation_chooser,
            field_count: self.field_count,
            insert_order: self.insert_order,
            key_chooser: match self.request_distribution {
                number::Distribution::Constant => unreachable!(),
                number::Distribution::Uniform => generator::Number::uniform(keys_total),
                number::Distribution::Zipfian => generator::Number::zipfian(keys_total),
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

#[derive(Copy, Clone, PartialEq, Eq)]
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
    pub fn next_key<R: Rng>(&mut self, rng: &mut R) -> Key {
        let mut key = self.key_chooser.next(rng);
        let max = self.record_count as u64 + self.acked.max();
        while key >= max {
            key = self.key_chooser.next(rng);
        }
        Key::new(self.insert_order, key)
    }

    #[inline]
    pub fn next_field<R: Rng>(&mut self, rng: &mut R) -> u64 {
        self.field_chooser.next(rng)
    }

    /// Only newly inserted keys can be acknowledged
    #[inline]
    pub fn acknowledge(&self, key: Key) {
        self.acked
            .acknowledge(key.sequence() - self.record_count as u64);
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
