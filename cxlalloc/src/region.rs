pub(crate) mod data;
pub(crate) use data::Data;

// Namespace modules under `meta`, but avoid another
// layer of nesting in the file hierarchy.
#[path = "region"]
pub(crate) mod meta {
    const MAX_THREAD_COUNT: usize = 64;

    pub(crate) mod owned;
    pub(crate) mod shared;

    pub(crate) use owned::Owned;
    pub(crate) use shared::Shared;
}
