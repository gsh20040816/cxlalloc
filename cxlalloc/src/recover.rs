use crate::allocator::Allocator;
use crate::allocator::Context;
use crate::atomic::Version;
use crate::bitset::Bit;
use crate::size;
use crate::slab;
use crate::view;

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
            HeapStateUnpacked::GlobalToLocal(_state) => todo!(),
            HeapStateUnpacked::BumpToLocal(_state) => todo!(),
            HeapStateUnpacked::LocalToGlobal(_state) => todo!(),
            HeapStateUnpacked::SizedToApplication(_state) => todo!(),
            HeapStateUnpacked::ApplicationToSized(_state) => todo!(),
            HeapStateUnpacked::LocalToGlobalSave(_state) => todo!(),
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
    GlobalToLocal {
        #[ribbit(size = 32)]
        index: slab::Index<B>,

        #[ribbit(size = 16)]
        version: Version,
    },

    #[ribbit(size = 48, from)]
    BumpToLocal {
        #[ribbit(size = 32)]
        start: Option<slab::Index<B>>,

        #[ribbit(size = 16)]
        version: Version,
    },

    #[ribbit(size = 48, from)]
    LocalToGlobal {
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

    #[ribbit(size = 32, from)]
    LocalToGlobalSave {
        #[ribbit(size = 32)]
        index: slab::Index<B>,
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
