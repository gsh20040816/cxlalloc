use rand::Rng;

mod discrete;

pub use discrete::Discrete;

pub trait Generator {
    type Item;
    fn next<R: Rng>(&mut self, rng: &mut R) -> Self::Item;
}
