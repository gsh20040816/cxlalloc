use core::marker::PhantomData;

use crate::allocator;
use crate::atomic::Version;
use crate::cache;
use crate::cas;
use crate::recover;
use crate::recover::HeapState;
use crate::size;
use crate::slab::Index;
use crate::slab::Slab;
use crate::thread;

#[repr(C)]
pub(crate) struct Local<B> {
    head: Option<Index<B>>,
    count: usize,
    _bracket: PhantomData<B>,
}

impl<B: size::Bracket> Local<B> {
    pub(crate) fn peek(&self) -> Option<Index<B>> {
        self.head
    }

    pub(crate) fn len(&self) -> usize {
        self.count
    }

    pub(crate) fn set(&mut self, head: Option<Index<B>>, count: usize) {
        self.head = head;
        cache::flush(&self.head, cache::Invalidate::No);

        self.count = count;
    }

    pub(crate) fn pop(&mut self, slabs: &Slab<B>) -> Option<Index<B>> {
        let head = self.head?;
        self.head = slabs.local(head).next.load();
        cache::flush(&self.head, cache::Invalidate::No);

        self.count -= 1;
        Some(head)
    }

    pub(crate) fn push(&mut self, slabs: &Slab<B>, index: Index<B>) {
        if self.head == Some(index) {
            return;
        }

        let head = slabs.local(index);
        head.next.store(self.head);
        cache::flush(&head.next, cache::Invalidate::No);

        self.head = Some(index);
        cache::flush(&self.head, cache::Invalidate::No);

        self.count += 1;
    }

    pub(crate) fn trace<'a>(&self, slabs: &'a Slab<B>) -> impl Iterator<Item = Index<B>> + 'a {
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
    B: size::Bracket,
    recover::State: From<HeapState<B>>,
{
    pub(crate) fn push(
        &self,
        context: &mut allocator::Context,
        slabs: &Slab<B>,
        head: Index<B>,
        tail: Index<B>,
    ) {
        self.head
            .update(context, |old, version| {
                slabs.local(tail).next.store(old);
                cache::flush(&slabs.local(tail).next, cache::Invalidate::No);
                Some((
                    Some(head),
                    recover::LocalToGlobal::new(head, version).into(),
                ))
            })
            .unwrap();
    }

    pub(crate) fn pop(
        &self,
        context: &mut allocator::Context,
        slabs: &Slab<B>,
    ) -> Option<Index<B>> {
        self.head
            .update(context, |old, version| {
                let old = old?;
                let new = slabs.local(old).next.load();
                Some((new, recover::GlobalToLocal::new(old, version).into()))
            })
            .flatten()
    }

    pub(crate) fn is_empty(&self, context: &allocator::Context) -> bool {
        self.head.load(context).is_none()
    }
}

#[ribbit::pack(size = 64, debug, eq)]
struct Head<B> {
    #[ribbit(size = 16, nonzero)]
    id: thread::Id,

    #[ribbit(size = 16)]
    version: Version,

    #[ribbit(size = 32)]
    index: Option<Index<B>>,
}
