pub struct Heap<'raw, B> {
    /// Multiple-reader, multiple-writer metadata
    pub(crate) shared: &'raw Shared,

    /// Single-reader, single-writer metadata
    pub(crate) owned: &'raw Owned,

    pub(crate) slabs: region::Slab<'raw, B>,
    pub(crate) data: region::Data<'raw, B>,
}

impl<B> Heap<'_, B> {}
