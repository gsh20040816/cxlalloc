#[ribbit::pack(size = 8, new(vis = ""))]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Epoch(u8);

impl Epoch {
    /// The total size of all epochs up to and including this one in bytes.
    pub fn total_byte(&self, initial: usize) -> usize {
        2usize.pow(self._0() as u32) * initial
    }

    /// The total size of all epochs up to and including this one in slabs.
    pub(crate) fn total(&self, initial: u32) -> u32 {
        2u32.pow(self._0() as u32) * initial
    }

    /// The offset of this last epoch.
    pub(crate) fn offset(&self, initial: u32) -> u32 {
        match self._0() {
            0 => 0,
            _ => Epoch::new(self._0() - 1).total(initial),
        }
    }

    /// The size of this last epoch.
    pub(crate) fn partial(&self, initial: u32) -> u32 {
        match self._0() {
            0 => initial,
            _ => Epoch::new(self._0() - 1).total(initial),
        }
    }

    pub(crate) fn next(&self) -> Self {
        Self::new(self._0() + 1)
    }
}

impl From<Epoch> for u8 {
    fn from(epoch: Epoch) -> Self {
        epoch._0()
    }
}
