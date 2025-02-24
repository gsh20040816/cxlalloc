use rand::distr::Distribution;
use rand::distr::weighted::WeightedIndex;

use super::Generator;

pub struct Discrete<T> {
    weights: WeightedIndex<f32>,
    values: Vec<T>,
}

impl<T> Discrete<T> {
    #[inline]
    pub fn new(choices: Vec<(T, f32)>) -> Self {
        let weights = WeightedIndex::new(choices.iter().map(|(_, weight)| *weight)).unwrap();
        let values = choices.into_iter().map(|(value, _)| value).collect();
        Self { weights, values }
    }
}

impl<T> Generator for Discrete<T>
where
    T: Copy,
{
    type Item = T;

    #[inline]
    fn next<R: rand::Rng>(&mut self, rng: &mut R) -> Self::Item {
        let index = self.weights.sample(rng);
        self.values[index]
    }
}
