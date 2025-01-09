use crate::view;

pub struct Heap<'raw, B> {
    /// Multiple-reader, multiple-writer metadata
    pub(crate) shared: &'raw view::Shared,

    /// Single-reader, single-writer metadata
    pub(crate) owned: &'raw view::owned::Array,

    pub(crate) slabs: view::Slab<'raw, B>,
    pub(crate) data: view::Data<'raw, B>,
}

impl<B> Heap<'_, B> {}
