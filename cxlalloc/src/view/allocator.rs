use crate::cas;
use crate::log;
use crate::size;
use crate::thread;
use crate::view;
use crate::view::data;
use crate::Atomic;

pub struct Allocator<'raw, L: view::Lens> {
    id: thread::Id,

    shared: &'raw Shared,
    owned: L::Scope<'raw, Owned>,

    small: view::Heap<'raw, L, size::Small>,
    huge: view::Huge<'raw>,
}

impl<'raw, L: view::Lens> Allocator<'raw, L> {
    pub(crate) fn new(
        id: thread::Id,
        shared: &'raw Shared,
        owned: L::Scope<'raw, Owned>,
        small: view::Heap<'raw, L, size::Small>,
        huge: view::Huge<'raw>,
    ) -> Self {
        Self {
            id,
            shared,
            owned,
            small,
            huge,
        }
    }

    pub(crate) unsafe fn focus(self, id: thread::Id) -> Allocator<'raw, view::Focus> {
        Allocator {
            id,
            shared: self.shared,
            owned: L::focus(self.owned, id),
            small: self.small.focus(id),
            huge: self.huge,
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
