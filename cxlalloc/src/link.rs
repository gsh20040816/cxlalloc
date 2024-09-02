use core::ptr::NonNull;

use crate::region;
use crate::root;
use crate::Heap;

pub enum Site {
    Root(root::Index),
    Data(region::data::Offset),
}

pub trait Erase<'raw, 'root, T>: Sized {
    fn erase(self, heap: &Heap<'raw>) -> Site;
}

impl<'raw, 'root, T> Erase<'raw, 'root, T> for &'root mut Option<crate::Box<T>> {
    fn erase(self, heap: &Heap<'raw>) -> Site {
        Site::Data(heap.pointer_to_offset(NonNull::from(self)))
    }
}

impl<'raw, 'root, T> Erase<'raw, 'root, T> for &'root mut crate::Root<'raw, T> {
    fn erase(self, _: &Heap<'raw>) -> Site {
        Site::Root(self.index())
    }
}
