use std::io;

use crate::raw::backend;
use crate::raw::Backend;
use crate::Raw;

pub struct Builder {
    pub(super) backend: Backend,
    pub(super) size: usize,
    pub(super) thread_count: usize,
    pub(super) free: bool,
}

impl Builder {
    pub fn build(self, id: &str) -> io::Result<Raw> {
        Raw::new(id, self)
    }

    pub fn backend<B: Into<Backend>>(mut self, backend: B) -> Self {
        self.backend = backend.into();
        self
    }

    pub fn size(mut self, size: usize) -> Self {
        self.size = size;
        self
    }

    pub fn thread_count(mut self, thread_count: usize) -> Self {
        self.thread_count = thread_count;
        self
    }

    pub fn free(mut self, free: bool) -> Self {
        self.free = free;
        self
    }
}

impl Default for Builder {
    fn default() -> Self {
        Builder {
            backend: Backend::Mmap(backend::Mmap),
            size: 0,
            thread_count: 1,
            free: false,
        }
    }
}
