mod linear_hash_map;
mod linked_hash_map;

use bon::Builder;
pub use linear_hash_map::LinearHashMap;
pub use linked_hash_map::LinkedHashMap;

use core::hash::Hash;
use core::mem;
use core::mem::MaybeUninit;
use core::ptr;
use std::io;

use serde::Deserialize;
use serde::Serialize;

use crate::Allocator;

#[derive(Builder, Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    /// Whether to inline the value into index entries (or else allocate separately)
    pub(crate) inline: bool,

    /// Size of hash map backing array
    pub(crate) len: usize,

    /// Whether to map populate the index
    pub(crate) populate: bool,
}

pub trait Key: Hash + PartialEq {
    fn len(&self) -> usize;
    unsafe fn copy(&self, buffer: &mut [MaybeUninit<u8>]);
    unsafe fn from_ptr(pointer: *mut u8) -> Self;
}

impl Key for u64 {
    fn len(&self) -> usize {
        mem::size_of::<Self>()
    }

    unsafe fn copy(&self, buffer: &mut [MaybeUninit<u8>]) {
        unsafe {
            ptr::copy_nonoverlapping(self.to_ne_bytes().as_ptr(), buffer.as_mut_ptr().cast(), 8)
        }
    }

    unsafe fn from_ptr(pointer: *mut u8) -> Self {
        unsafe { pointer.cast::<Self>().read() }
    }
}

pub trait Index<A, K>
where
    Self: Sized,
    A: Allocator,
    K: Key,
{
    fn new(numa: Option<usize>, name: &str, len: usize, populate: bool) -> io::Result<Self>;

    fn unlink(&mut self) -> io::Result<()>;

    fn insert<F: FnOnce(&mut A, *mut u8)>(&self, allocator: &mut A, key: K, size: usize, with: F);

    fn get<F: FnOnce(&mut A, *const u8)>(&self, allocator: &mut A, key: K, with: F) -> bool;
}
