use crate::region;

pub struct Heap<'raw, B> {
    /// Multiple-reader, multiple-writer metadata
    pub(crate) shared: &'raw region::Shared,

    /// Single-reader, single-writer metadata
    pub(crate) owned: &'raw region::Owned,

    pub(crate) slabs: region::Slab<'raw, B>,
    pub(crate) data: region::Data<'raw, B>,
}

impl<B> Heap<'_, B> {}
