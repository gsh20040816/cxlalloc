use rand::distr::Distribution as _;

use crate::generator::Generator;

#[derive(Debug)]
pub enum Number {
    Constant(u64),
    Uniform(rand::distr::Uniform<u64>),
    Zipfian {
        max: f64,
        cutoff_1: f64,
        alpha: f64,
        eta: f64,
        zeta: f64,
    },
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
        const ZIPFIAN_CONSTANT: f64 = 0.99;
        let item_count = max + 1;
        let theta = ZIPFIAN_CONSTANT;
        let alpha = 1.0 / (1.0 - theta);

        let zeta_n = zeta_static(item_count, theta);
        let zeta_2 = zeta_static(2, theta);
        let eta = (1.0 - (2.0 / item_count as f64).powf(1.0 - theta)) / (1.0 - zeta_2 / zeta_n);

        Self::Zipfian {
            max: max as f64,
            cutoff_1: 1.0 + 0.5f64.powf(theta),
            alpha,
            eta,
            zeta: zeta_n,
        }
    }
}

fn zeta_static(n: u64, theta: f64) -> f64 {
    (1..=n).map(|i| i as f64).map(|i| 1.0 / i.powf(theta)).sum()
}

impl Generator for Number {
    type Item = u64;

    #[inline]
    fn next<R: rand::Rng>(&mut self, rng: &mut R) -> Self::Item {
        match self {
            Number::Constant(value) => *value,
            Number::Uniform(uniform) => uniform.sample(rng),
            Number::Zipfian {
                max,
                cutoff_1,
                alpha,
                eta,
                zeta,
            } => {
                let u = rng.random::<f64>();
                let uz = u * *zeta;
                if uz < 1.0 {
                    return 0;
                }

                if uz < *cutoff_1 {
                    return 1;
                }

                (*max * (*eta * (u - 1.0) + 1.0).powf(*alpha)) as u64
            }
        }
    }
}
