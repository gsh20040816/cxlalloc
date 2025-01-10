use core::cell::UnsafeCell;

use crate::cas;
use crate::log;
use crate::size;
use crate::thread;
use crate::view;
use crate::view::data;
use crate::Atomic;

pub struct Allocator<'raw> {
    shared: &'raw Shared,
    owned: &'raw thread::Array<UnsafeCell<Owned>>,

    small: view::Heap<'raw, size::Small>,
    huge: view::Huge<'raw>,
}

impl<'raw> Allocator<'raw> {
    pub(crate) fn new(
        shared: &'raw Shared,
        owned: &'raw thread::Array<UnsafeCell<Owned>>,
        small: view::Heap<'raw, size::Small>,
        huge: view::Huge<'raw>,
    ) -> Self {
        Self {
            shared,
            owned,
            small,
            huge,
        }
    }
}

#[repr(C)]
pub(crate) struct Shared {
    root: Atomic<Option<data::Offset>>,
    help: cas::help::Array,
}

#[repr(C)]
pub(crate) struct Owned {
    root: Option<data::Offset>,
    state: log::State,
}
