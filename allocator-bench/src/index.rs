mod linear_hash_map;
mod linked_hash_map;

use bon::Builder;
pub use linear_hash_map::LinearHashMap;
pub use linked_hash_map::LinkedHashMap;

use std::io;

use serde::Deserialize;
use serde::Serialize;

use crate::Allocator;

#[derive(Builder, Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    /// Size of hash map backing array
    pub(crate) len: usize,

    /// Whether to map populate the index
    pub(crate) populate: bool,
}

pub trait Index<A>
where
    Self: Sized,
    A: Allocator,
{
    fn new(numa: Option<usize>, name: &str, len: usize, populate: bool) -> io::Result<Self>;

    fn unlink(&mut self) -> io::Result<()>;

    fn insert<F: FnOnce(*mut u8)>(&self, allocator: &mut A, key: &[u8], size: usize, with: F);

    fn get<F: FnOnce(&mut A, *const u8)>(&self, allocator: &mut A, key: &[u8], with: F) -> bool;
}
