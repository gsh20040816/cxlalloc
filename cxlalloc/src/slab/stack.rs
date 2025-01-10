use crate::atomic::Version;
use crate::cas;
use crate::cas::help;
use crate::log;
use crate::slab::Index;
use crate::slab::Owned;
use crate::slab::Slice;
use crate::thread;

#[repr(C)]
pub(crate) struct LocalStack {
    head: Option<Index>,
    count: usize,
}

impl LocalStack {
    pub(crate) fn peek(&self) -> Option<Index> {
        self.head
    }

    pub(crate) fn len(&self) -> usize {
        self.count
    }

    pub(crate) fn set(&mut self, head: Option<Index>, count: usize) {
        self.count = count;
        self.head = head;
        crate::flush(&self, false);
    }

    pub(crate) fn pop(&mut self, slabs: &Slice<Owned>) -> Option<Index> {
        let index = self.head?;
        self.count -= 1;
        self.head = slabs[index].next.load();
        crate::flush(self, false);
        Some(index)
    }

    pub(crate) fn push(&mut self, slabs: &Slice<Owned>, index: Index) {
        let slab = &slabs[index];
        slab.next.store(self.head);
        crate::flush(&slab.next, false);

        self.count += 1;
        self.head = Some(index);
        crate::flush(&self.head, false);
    }

    pub(crate) fn trace<'a>(&self, slabs: &'a Slice<Owned>) -> impl Iterator<Item = Index> + 'a {
        slabs.trace(self.head)
    }
}

#[repr(C)]
pub(crate) struct GlobalStack {
    head: cas::Detectable<Option<Index>>,
}

impl GlobalStack {
    pub(crate) fn push(
        &self,
        id: thread::Id,
        slabs: &Slice<Owned>,
        help: &help::Array,
        head: Index,
        tail: Index,
    ) {
        self.head.update(help, id, |old, version| {
            slabs[tail].next.store(old);
            crate::flush(&slabs[tail].next, false);
            Some((
                Some(head),
                log::StateUnpacked::LocalToGlobal(log::LocalToGlobal::new(head, version)),
            ))
        });
    }

    pub(crate) fn pop(
        &self,
        id: thread::Id,
        slabs: &Slice<Owned>,
        help: &help::Array,
    ) -> Option<Index> {
        self.head
            .update(help, id, |old, version| {
                let old = old?;
                let new = slabs[old].next.load();

                Some((
                    new,
                    log::StateUnpacked::GlobalToLocal(log::GlobalToLocal::new(old, version)),
                ))
            })
            .flatten()
    }

    pub(crate) fn is_empty(&self, help: &help::Array) -> bool {
        self.head.load(help).is_none()
    }
}

#[ribbit::pack(size = 64, debug)]
#[derive(Copy, Clone, PartialEq, Eq)]
struct Head {
    #[ribbit(size = 16, nonzero)]
    id: thread::Id,

    #[ribbit(size = 16)]
    version: Version,

    #[ribbit(size = 32)]
    index: Option<Index>,
}
