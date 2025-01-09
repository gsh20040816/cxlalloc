use core::cell::UnsafeCell;

use crate::cas;
use crate::log;
use crate::size;
use crate::thread;
use crate::view;
use crate::view::data;
use crate::Atomic;

pub(crate) struct Shared {
    root: Atomic<Option<data::Offset>>,
    help: cas::help::Array,
}

pub(crate) struct Owned {
    root: Option<data::Offset>,
    state: log::State,
}

pub struct Allocator<'raw> {
    shared: &'raw Shared,
    owned: &'raw thread::Array<UnsafeCell<Owned>>,

    small: view::Heap<'raw, size::Class>,
    huge: view::Huge<'raw>,
}
