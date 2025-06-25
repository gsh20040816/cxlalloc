use core::ops::Deref;
use core::ops::DerefMut;

#[repr(C)]
#[ribbit::pack(size = 16, debug, eq, hash)]
pub struct Version(u16);

impl Default for Version {
    fn default() -> Self {
        Self::new(0)
    }
}

impl Version {
    pub fn next(&self) -> Self {
        Self::new(self._0().wrapping_add(1))
    }
}

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
