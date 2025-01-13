use core::marker::PhantomData;

use crate::allocator;
use crate::atomic::Version;
use crate::cas;
use crate::cas::help;
use crate::recover;
use crate::slab::Index;
use crate::slab::Slice;
use crate::thread;

#[repr(C)]
pub(crate) struct Local<B> {
    head: Option<Index<B>>,
    count: usize,
    _bracket: PhantomData<B>,
}

impl<B> Local<B> {
    pub(crate) fn peek(&self) -> Option<Index<B>> {
        self.head
    }

    pub(crate) fn len(&self) -> usize {
        self.count
    }

    pub(crate) fn set(&mut self, head: Option<Index<B>>, count: usize) {
        self.count = count;
        self.head = head;
        crate::flush(&self, false);
    }

    pub(crate) fn pop(&mut self, slabs: &Slice<B>) -> Option<Index<B>> {
        let index = self.head?;
        self.count -= 1;
        self.head = slabs[index].local.next.load();
        crate::flush(self, false);
        Some(index)
    }

    pub(crate) fn push(&mut self, slabs: &Slice<B>, index: Index<B>) {
        let slab = &slabs[index].local;
        slab.next.store(self.head);
        crate::flush(&slab.next, false);

        self.count += 1;
        self.head = Some(index);
        crate::flush(&self.head, false);
    }

    pub(crate) fn trace<'a>(&self, slabs: &'a Slice<B>) -> impl Iterator<Item = Index<B>> + 'a {
        slabs.trace(self.head)
    }
}

#[repr(C)]
pub(crate) struct Global<B> {
    head: cas::Detectable<Option<Index<B>>>,
    _bracket: PhantomData<B>,
}

impl<B> Global<B>
where
    Index<B>: Into<allocator::Index>,
{
    pub(crate) fn push(
        &self,
        context: &mut allocator::Context,
        slabs: &Slice<B>,
        head: Index<B>,
        tail: Index<B>,
    ) {
        self.head.update(context, |old, version| {
            slabs[tail].local.next.store(old);
            crate::flush(&slabs[tail].local.next, false);
            Some((
                Some(head),
                recover::StateUnpacked::LocalToGlobal(recover::LocalToGlobal::new(
                    head.into(),
                    version,
                )),
            ))
        });
    }

    pub(crate) fn pop(
        &self,
        context: &mut allocator::Context,
        slabs: &Slice<B>,
    ) -> Option<Index<B>> {
        self.head
            .update(context, |old, version| {
                let old = old?;
                let new = slabs[old].local.next.load();

                Some((
                    new,
                    recover::StateUnpacked::GlobalToLocal(recover::GlobalToLocal::new(
                        old.into(),
                        version,
                    )),
                ))
            })
            .flatten()
    }

    pub(crate) fn is_empty(&self, help: &help::Array) -> bool {
        self.head.load(help).is_none()
    }
}

#[ribbit::pack(size = 64, debug)]
#[derive(PartialEq, Eq)]
struct Head<B> {
    #[ribbit(size = 16, nonzero)]
    id: thread::Id,

    #[ribbit(size = 16)]
    version: Version,

    #[ribbit(size = 32)]
    index: Option<Index<B>>,
}

impl<B> Clone for Head<B> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<B> Copy for Head<B> {}
