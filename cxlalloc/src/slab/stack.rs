use core::marker::PhantomData;
use core::sync::atomic::Ordering;

use crate::allocator;
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
    len: usize,
    _bracket: PhantomData<B>,
}

impl<B: size::Bracket> Local<B> {
    // Invariant: `len` = `trace(head).count()`
    // Implies no cycles (or else `trace(..)` is infinite loop)
    pub(crate) fn is_valid(&self, slabs: &Slab<B>) -> bool {
        let mut i = 0;
        for _ in self.trace(slabs) {
            i += 1;
            if i > self.len {
                return false;
            }
        }
        i == self.len
    }

    pub(crate) fn peek(&self) -> Option<Index<B>> {
        self.head
    }

    pub(crate) fn len(&self) -> usize {
        self.len
    }

    pub(crate) fn set(&mut self, head: Option<Index<B>>, len: usize) {
        self.head = head;
        cache::flush(&self.head, cache::Invalidate::No);

        self.len = len;
    }

    pub(crate) fn pop(&mut self, slabs: &Slab<B>) -> Option<Index<B>> {
        let head = self.head?;
        self.head = slabs.local(head).next.load(Ordering::Relaxed);
        cache::flush(&self.head, cache::Invalidate::No);

        self.len -= 1;
        Some(head)
    }

    pub(crate) fn push(&mut self, slabs: &Slab<B>, index: Index<B>) {
        let head = slabs.local(index);
        head.next.store(self.head, Ordering::Relaxed);
        cache::flush(&head.next, cache::Invalidate::No);

        // Prevent reordering to guarantee that `head` points to `self.head`
        cache::fence();

        self.head = Some(index);
        cache::flush(&self.head, cache::Invalidate::No);

        // Count can be recomputed on recovery and doesn't
        // require flushing or fencing.
        self.len += 1;
    }

    pub(crate) fn recover_push(&mut self, slabs: &Slab<B>, index: Index<B>) {
        if self.head != Some(index) {
            self.push(slabs, index);
        }

        self.recover_len(slabs);
    }

    pub(crate) fn recover_len(&mut self, slabs: &Slab<B>) {
        self.len = self.trace(slabs).count();
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
            .update(
                context,
                Ordering::AcqRel,
                Ordering::Acquire,
                |old, version| {
                    slabs.local(tail).next.store(old, Ordering::Relaxed);
                    cache::flush(&slabs.local(tail).next, cache::Invalidate::No);
                    Some((
                        Some(head),
                        recover::HeapState::UnsizedToGlobal {
                            index: head,
                            version,
                        },
                    ))
                },
            )
            .unwrap();
    }

    pub(crate) fn pop(
        &self,
        context: &mut allocator::Context,
        slabs: &Slab<B>,
    ) -> Option<Index<B>> {
        self.head
            .update(
                context,
                Ordering::AcqRel,
                Ordering::Acquire,
                |old, version| {
                    let old = old?;
                    let new = slabs.local(old).next.load(Ordering::Relaxed);
                    Some((
                        new,
                        recover::HeapState::GlobalToUnsized {
                            index: old,
                            version,
                        },
                    ))
                },
            )
            .flatten()
    }

    pub(crate) fn detect(&self, context: &mut allocator::Context, version: cas::Version) -> bool {
        self.head.detect(context, version)
    }

    pub(crate) fn is_empty(&self, context: &allocator::Context) -> bool {
        self.head.load(context, Ordering::Relaxed).is_none()
    }
}

#[derive(ribbit::Pack)]
#[ribbit(size = 64)]
struct Head<B> {
    #[ribbit(size = 16, nonzero)]
    id: thread::Id,

    #[ribbit(size = 16)]
    version: cas::Version,

    #[ribbit(size = 32)]
    index: Option<Index<B>>,
}

impl<B> Copy for Head<B> {}
impl<B> Clone for Head<B> {
    fn clone(&self) -> Self {
        *self
    }
}
