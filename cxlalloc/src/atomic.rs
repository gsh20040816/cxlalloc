use core::ops::Deref;
use core::ops::DerefMut;

#[repr(align(64))]
pub struct Pad<T>(T);

impl<T> Deref for Pad<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for Pad<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
