use crate::raw;
use crate::thread;
use crate::Heap;

pub struct Allocator<'raw> {
    id: thread::Id,
    heap: Heap<'raw>,
}

impl<'raw> Allocator<'raw> {
    pub(crate) fn from_raw(raw: &'raw raw::Heap, id: thread::Id) -> Self {
        unsafe {
            Self {
                id,
                heap: Heap::from_raw(raw),
            }
        }
    }
}
