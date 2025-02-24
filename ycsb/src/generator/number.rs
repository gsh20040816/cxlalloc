use core::hash::Hash as _;
use core::hash::Hasher as _;

use rand_distr::Distribution as _;
use rand_distr::Zipf;
use rapidhash::RapidHasher;
use serde::Deserialize;

use crate::generator::Generator;

#[derive(Debug, Deserialize)]
pub enum Distribution {
    #[serde(alias = "constant")]
    Constant,

    #[serde(alias = "uniform")]
    Uniform,

    #[serde(alias = "zipfian")]
    Zipfian,
}

pub enum Number {
    Constant(u64),
    Uniform(rand::distr::Uniform<u64>),
    Zipfian { inner: Zipf<f32>, max: u64 },
}

impl Number {
    #[inline]
    pub fn constant(value: u64) -> Self {
        Self::Constant(value)
    }

    #[inline]
    pub fn uniform(max: u64) -> Self {
        Self::Uniform(rand::distr::Uniform::new(0, max).unwrap())
    }

    #[inline]
    pub fn zipfian(max: u64) -> Self {
        Self::Zipfian {
            inner: Zipf::new(max as f32, 2.0).unwrap(),
            max,
        }
    }
}

impl Generator for Number {
    type Item = u64;

    #[inline]
    fn next<R: rand::Rng>(&mut self, rng: &mut R) -> Self::Item {
        match self {
            Number::Constant(value) => *value,
            Number::Uniform(uniform) => uniform.sample(rng),
            Number::Zipfian { inner, max } => {
                let key = inner.sample(rng) as u64;
                let mut hasher = RapidHasher::default();
                key.hash(&mut hasher);
                hasher.finish() % *max
            }
        }
    }
}
