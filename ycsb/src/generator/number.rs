use rand_distr::Distribution as _;
use rand_distr::Zipf;
use serde::Deserialize;

use crate::generator::Generator;

pub enum Number {
    Constant(u64),
    Uniform(rand::distr::Uniform<u64>),
    Zipfian(Zipf<f32>),
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
        Self::Zipfian(Zipf::new(max as f32, 2.0).unwrap())
    }
}

impl Generator for Number {
    type Item = u64;

    #[inline]
    fn next<R: rand::Rng>(&mut self, rng: &mut R) -> Self::Item {
        match self {
            Number::Constant(value) => *value,
            Number::Uniform(uniform) => uniform.sample(rng),
            Number::Zipfian(zipfian) => zipfian.sample(rng) as u64,
        }
    }
}
