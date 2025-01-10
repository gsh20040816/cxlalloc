use crate::cas;
use crate::log;
use crate::size;
use crate::thread;
use crate::view;
use crate::view::data;
use crate::Atomic;

pub struct Allocator<'raw, L: view::Lens> {
    pub(crate) id: thread::Id,

    pub(crate) shared: &'raw Shared,
    pub(crate) owned: L::Scope<'raw, Owned>,

    pub(crate) small: view::Heap<'raw, L, size::Small>,
    pub(crate) huge: view::Huge<'raw>,
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
    pub(crate) root: Atomic<Option<data::Offset>>,
    pub(crate) help: cas::help::Array,
}

#[repr(C, align(64))]
pub(crate) struct Owned {
    pub(crate) root: Option<data::Offset>,
    pub(crate) state: log::State,
}
