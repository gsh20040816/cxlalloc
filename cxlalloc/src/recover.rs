use crate::allocator::Allocator;
use crate::allocator::Bracket;
use crate::allocator::BracketUnpacked;
use crate::allocator::Context;
use crate::allocator::Index;
use crate::allocator::IndexUnpacked;
use crate::atomic::Version;
use crate::bitset::Bit;
use crate::heap;
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
            StateUnpacked::UnsizedToSized(state) => match state.class().unpack() {
                BracketUnpacked::Small(class) => {
                    let r#unsized = &mut self.small.owned.r#unsized;

                    let index =
                        state
                            .index()
                            .map(|index| index.unpack())
                            .map(|index| match index {
                                IndexUnpacked::Small(index) => index,
                            });

                    let slabs = &self.small.slabs;

                    match r#unsized.peek() {
                        // Successfully pushed to `r#sized`
                        head if head != index => {
                            let count = r#unsized.trace(&self.small.slabs).count();
                            r#unsized.set(head, count);
                        }
                        // Retry
                        _ => {
                            self.small.owned.unsized_to_sized(context, slabs, class);
                        }
                    }
                }
            },
            StateUnpacked::GlobalToLocal(state) => todo!(),
            StateUnpacked::BumpToLocal(state) => todo!(),
            StateUnpacked::LocalToGlobal(state) => todo!(),
            StateUnpacked::SizedToApplication(state) => todo!(),
            StateUnpacked::ApplicationToSized(state) => todo!(),
            StateUnpacked::LocalToGlobalSave(state) => todo!(),
            StateUnpacked::Remote(state) => todo!(),
        }
    }
}

#[ribbit::pack(size = 64, nonzero)]
#[derive(Copy, Clone)]
pub(crate) enum State {
    #[ribbit(size = 40)]
    #[derive(Copy, Clone)]
    UnsizedToSized {
        #[ribbit(size = 32)]
        index: Option<Index>,

        #[ribbit(size = 8)]
        class: Bracket,
    },

    #[ribbit(size = 48)]
    #[derive(Copy, Clone)]
    GlobalToLocal {
        #[ribbit(size = 32)]
        index: Index,

        #[ribbit(size = 16)]
        version: Version,
    },

    #[ribbit(size = 48)]
    #[derive(Copy, Clone)]
    BumpToLocal {
        #[ribbit(size = 32)]
        bump: heap::Bump,

        #[ribbit(size = 16)]
        version: Version,
    },

    #[ribbit(size = 48)]
    #[derive(Copy, Clone)]
    LocalToGlobal {
        #[ribbit(size = 32)]
        index: Index,

        #[ribbit(size = 16)]
        version: Version,
    },

    #[ribbit(size = 44)]
    #[derive(Copy, Clone)]
    SizedToApplication {
        #[ribbit(size = 32)]
        index: Index,

        #[ribbit(size = 12)]
        block: Bit,
    },

    #[ribbit(size = 44)]
    #[derive(Copy, Clone)]
    ApplicationToSized {
        #[ribbit(size = 32)]
        index: Index,

        #[ribbit(size = 12)]
        block: Bit,
    },

    #[ribbit(size = 32)]
    #[derive(Copy, Clone)]
    LocalToGlobalSave {
        #[ribbit(size = 32)]
        index: Index,
    },

    #[ribbit(size = 60)]
    #[derive(Copy, Clone)]
    Remote {
        #[ribbit(size = 32)]
        index: Index,

        #[ribbit(size = 12)]
        block: Bit,

        #[ribbit(size = 16)]
        version: Version,
    },
}
