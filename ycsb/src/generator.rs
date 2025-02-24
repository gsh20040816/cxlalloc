use rand::Rng;

mod discrete;
pub mod number;

pub use discrete::Discrete;
pub use number::Number;

pub trait Generator {
    type Item;
    fn next<R: Rng>(&mut self, rng: &mut R) -> Self::Item;
}
