use core::sync::atomic::Ordering;

use crate::allocator::Allocator;
use crate::allocator::Context;
use crate::atomic::Version;
use crate::bitset::Bit;
use crate::size;
use crate::slab;
use crate::view;
use crate::BATCH_BUMP_POP;

impl<S, O> Allocator<'_, view::Focus, S, O> {
    pub(crate) fn recover(&mut self) {
        let context = &mut Context {
            id: self.id,
            help: &self.shared.help,
            log: &mut self.owned.state,
        };

        let Some(state) = context.log else { return };

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
            HeapStateUnpacked::UnsizedToGlobal(_state) => todo!(),
            HeapStateUnpacked::SizedToApplication(_state) => todo!(),
            HeapStateUnpacked::ApplicationToSized(_state) => todo!(),
            HeapStateUnpacked::Remote(_state) => todo!(),
            HeapStateUnpacked::Detach(_state) => todo!(),
        }
    }
}

#[ribbit::pack(size = 64, nonzero, from)]
pub(crate) enum State {
    #[ribbit(size = 60)]
    Small(HeapState<size::Small>),
    #[ribbit(size = 60)]
    Large(HeapState<size::Large>),
}

#[ribbit::pack(size = 60, from)]
pub(crate) enum HeapState<B> {
    #[ribbit(size = 40, from)]
    UnsizedToSized {
        #[ribbit(size = 32)]
        index: Option<slab::Index<B>>,

        #[ribbit(size = 8)]
        class: B,
    },

    #[ribbit(size = 48, from)]
    GlobalToUnsized {
        #[ribbit(size = 32)]
        index: slab::Index<B>,

        #[ribbit(size = 16)]
        version: Version,
    },

    #[ribbit(size = 48, from)]
    BumpToUnsized {
        #[ribbit(size = 32)]
        start: Option<slab::Index<B>>,

        #[ribbit(size = 16)]
        version: Version,
    },

    #[ribbit(size = 32, from)]
    UnsizedToGlobalSave {
        #[ribbit(size = 32)]
        index: slab::Index<B>,
    },

    #[ribbit(size = 48, from)]
    UnsizedToGlobal {
        #[ribbit(size = 32)]
        index: slab::Index<B>,

        #[ribbit(size = 16)]
        version: Version,
    },

    #[ribbit(size = 44, from)]
    SizedToApplication {
        #[ribbit(size = 32)]
        index: slab::Index<B>,

        #[ribbit(size = 12)]
        block: Bit,
    },

    #[ribbit(size = 44, from)]
    ApplicationToSized {
        #[ribbit(size = 32)]
        index: slab::Index<B>,

        #[ribbit(size = 12)]
        block: Bit,
    },

    #[ribbit(size = 49, from)]
    Remote {
        #[ribbit(size = 32)]
        index: slab::Index<B>,

        #[ribbit(size = 16)]
        version: Version,

        last: bool,
    },

    #[ribbit(size = 56, from)]
    Detach {
        #[ribbit(size = 32)]
        index: slab::Index<B>,

        #[ribbit(size = 16)]
        version: Version,
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
