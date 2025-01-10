use core::marker::PhantomData;

use crate::atomic::Version;
use crate::cas;
use crate::cas::help;
use crate::log;
use crate::slab::Index;
use crate::slab::Slice;
use crate::thread;

#[repr(C)]
pub(crate) struct Local<B> {
    head: Option<Index>,
    count: usize,
    _bracket: PhantomData<B>,
}

impl<B> Local<B> {
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

    pub(crate) fn pop(&mut self, slabs: &Slice<B>) -> Option<Index> {
        let index = self.head?;
        self.count -= 1;
        self.head = slabs[index].local.next.load();
        crate::flush(self, false);
        Some(index)
    }

    pub(crate) fn push(&mut self, slabs: &Slice<B>, index: Index) {
        let slab = &slabs[index].local;
        slab.next.store(self.head);
        crate::flush(&slab.next, false);

        self.count += 1;
        self.head = Some(index);
        crate::flush(&self.head, false);
    }

    pub(crate) fn trace<'a>(&self, slabs: &'a Slice<B>) -> impl Iterator<Item = Index> + 'a {
        slabs.trace(self.head)
    }
}

#[repr(C)]
pub(crate) struct Global<B> {
    head: cas::Detectable<Option<Index>>,
    _bracket: PhantomData<B>,
}

impl<B> Global<B> {
    pub(crate) fn push(
        &self,
        id: thread::Id,
        slabs: &Slice<B>,
        help: &help::Array,
        head: Index,
        tail: Index,
    ) {
        self.head.update(help, id, |old, version| {
            slabs[tail].local.next.store(old);
            crate::flush(&slabs[tail].local.next, false);
            Some((
                Some(head),
                log::StateUnpacked::LocalToGlobal(log::LocalToGlobal::new(head, version)),
            ))
        });
    }

    pub(crate) fn pop(
        &self,
        id: thread::Id,
        slabs: &Slice<B>,
        help: &help::Array,
    ) -> Option<Index> {
        self.head
            .update(help, id, |old, version| {
                let old = old?;
                let new = slabs[old].local.next.load();

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
