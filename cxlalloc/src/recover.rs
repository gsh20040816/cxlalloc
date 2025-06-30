use core::sync::atomic::Ordering;

use crate::allocator::Allocator;
use crate::allocator::Context;
use crate::bitset::Bit;
use crate::cas;
use crate::size;
use crate::slab;
use crate::view;
use crate::BATCH_BUMP_POP;

impl<S, O> Allocator<'_, view::Focus, S, O> {
    pub(crate) fn recover(&mut self) {
        let context = &mut Context {
            id: self.id,
            help: &self.shared.help,
            log: &self.owned.state,
        };

        let Some(state) = context.log.load(Ordering::Relaxed) else {
            return;
        };

        dbg!(&state);

        match state.unpack() {
            StateUnpacked::Small(state) => Self::recover_heap(context, &mut self.small, state),
            StateUnpacked::Large(state) => Self::recover_heap(context, &mut self.large, state),
        }
    }

    fn recover_heap<B>(
        context: &mut Context,
        heap: &mut crate::Heap<view::Focus, B>,
        state: HeapState<B>,
    ) where
        B: size::Bracket,
        slab::Local<B>: slab::local::Cache<B>,
        State: From<HeapState<B>>,
    {
        match state.unpack() {
            HeapStateUnpacked::UnsizedToSized(state) => {
                let r#unsized = &mut heap.owned.r#unsized;
                let index = state.index();
                let slabs = &heap.slabs;

                match r#unsized.peek() {
                    // Successfully pushed to `r#sized`
                    head if head != index => {
                        let count = r#unsized.trace(&heap.slabs).count();
                        r#unsized.set(head, count);
                    }
                    // Retry
                    _ => {
                        heap.owned.unsized_to_sized(context, slabs, state.class());
                    }
                }
            }
            HeapStateUnpacked::GlobalToUnsized(state) => {
                let index = state.index();
                let version = state.version();

                // Crashed between logging and CASing
                if !heap.shared.detect_global(context, version) {
                    return;
                }

                heap.owned.r#unsized.recover_push(&heap.slabs, index);
            }
            // FIXME: deduplicate with `heap::Shared::bump`?
            HeapStateUnpacked::BumpToUnsized(state) => {
                let start = state.start().unwrap_or(slab::Index::MIN);
                let version = state.version();

                if !heap.shared.detect_bump(context, version) {
                    return;
                }

                let batch = BATCH_BUMP_POP.load(Ordering::Relaxed);
                let end = unsafe { start.add(batch as u32) };

                unsafe {
                    heap.slabs.link(start..end, None);
                    heap.owned.r#unsized.set(Some(start), batch);
                }
            }
            HeapStateUnpacked::UnsizedToGlobalSave(state) => {
                let index = state.index();

                match heap.owned.r#unsized.peek() {
                    // Crashed before popping batch from `r#unsized`
                    Some(head) if head == index => {
                        // Possible that writes to head and count were reordered,
                        // such that write to count persisted first before crash?
                        heap.owned.r#unsized.recover_count(&heap.slabs);
                    }
                    // Crashed after popping batch, undo
                    _ => {
                        heap.owned.r#unsized.set(Some(index), 0);
                        heap.owned.r#unsized.recover_count(&heap.slabs);
                    }
                }
            }
            HeapStateUnpacked::UnsizedToGlobal(state) => {
                let index = state.index();
                let version = state.version();

                // Completed successfully
                if heap.shared.detect_global(context, version) {
                    return;
                }

                // Undo popping of batch
                heap.owned.r#unsized.set(Some(index), 0);
                heap.owned.r#unsized.recover_count(&heap.slabs);
            }
            HeapStateUnpacked::SizedToApplication(_state) => todo!(),
            HeapStateUnpacked::ApplicationToSized(_state) => todo!(),
            HeapStateUnpacked::Remote(state) => {
                let index = state.index();
                let version = state.version();
                let last = state.last();

                let slab = heap.slabs.remote(index);

                // Crashed before CASing remote descriptor, retry
                if !slab.detect(context, version) {
                    heap.free_remote(context, index);
                    return;
                }

                // Finished CAS and do not need to claim slab
                if !last {
                    return;
                }

                heap.owned.r#unsized.recover_push(&heap.slabs, index);
                heap.unsized_to_global(context);
            }
            HeapStateUnpacked::Detach(state) => {
                let index = state.index();
                let version = state.version();

                let slab = heap.slabs.remote(index);
                let class = heap.slabs.local(index).get().class;

                if !slab.detect(context, version) {
                    heap.detach(context, class, index);
                }
            }
        }
    }
}

#[ribbit::pack(size = 64, nonzero, from, debug)]
pub(crate) enum State {
    #[ribbit(size = 60)]
    Small(HeapState<size::Small>),
    #[ribbit(size = 60)]
    Large(HeapState<size::Large>),
}

#[ribbit::pack(size = 60, from, debug)]
pub(crate) enum HeapState<B> {
    #[ribbit(size = 40, from, debug)]
    UnsizedToSized {
        #[ribbit(size = 32)]
        index: Option<slab::Index<B>>,

        #[ribbit(size = 8)]
        class: B,
    },

    #[ribbit(size = 48, from, debug)]
    GlobalToUnsized {
        #[ribbit(size = 32)]
        index: slab::Index<B>,

        #[ribbit(size = 16)]
        version: cas::Version,
    },

    #[ribbit(size = 48, from, debug)]
    BumpToUnsized {
        #[ribbit(size = 32)]
        start: Option<slab::Index<B>>,

        #[ribbit(size = 16)]
        version: cas::Version,
    },

    #[ribbit(size = 32, from, debug)]
    UnsizedToGlobalSave {
        #[ribbit(size = 32)]
        index: slab::Index<B>,
    },

    #[ribbit(size = 48, from, debug)]
    UnsizedToGlobal {
        #[ribbit(size = 32)]
        index: slab::Index<B>,

        #[ribbit(size = 16)]
        version: cas::Version,
    },

    #[ribbit(size = 44, from, debug)]
    SizedToApplication {
        #[ribbit(size = 32)]
        index: slab::Index<B>,

        #[ribbit(size = 12)]
        block: Bit,
    },

    #[ribbit(size = 44, from, debug)]
    ApplicationToSized {
        #[ribbit(size = 32)]
        index: slab::Index<B>,

        #[ribbit(size = 12)]
        block: Bit,
    },

    #[ribbit(size = 49, from, debug)]
    Remote {
        #[ribbit(size = 32)]
        index: slab::Index<B>,

        #[ribbit(size = 16)]
        version: cas::Version,

        last: bool,
    },

    #[ribbit(size = 56, from, debug)]
    Detach {
        #[ribbit(size = 32)]
        index: slab::Index<B>,

        #[ribbit(size = 16)]
        version: cas::Version,
    },
}

impl From<HeapState<size::Small>> for State {
    fn from(state: HeapState<size::Small>) -> Self {
        Self::new(StateUnpacked::Small(state))
    }
}

impl From<HeapState<size::Large>> for State {
    fn from(state: HeapState<size::Large>) -> Self {
        Self::new(StateUnpacked::Large(state))
    }
}
