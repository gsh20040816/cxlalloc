use crate::thread;
use crate::view::data;
use crate::Atomic;

pub(crate) struct Huge<'raw> {
    shared: &'raw Shared,
    owned: &'raw thread::Array<Owned>,
}

impl<'raw> Huge<'raw> {
    pub(crate) fn new(shared: &'raw Shared, owned: &'raw thread::Array<Owned>) -> Self {
        Self { shared, owned }
    }
}

pub(crate) struct Shared {
    slots: [Atomic<Option<thread::Id>>; 1024],
    next: Atomic<u64>,
}

pub(crate) struct Owned {
    head: Atomic<Option<data::Offset>>,
}
