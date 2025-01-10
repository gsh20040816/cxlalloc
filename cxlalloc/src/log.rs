use crate::atomic::Version;
use crate::bitset::Bit;
use crate::size;
use crate::slab;
use crate::view;

#[ribbit::pack(size = 64, nonzero)]
#[derive(Copy, Clone)]
pub(crate) enum State {
    #[ribbit(size = 40)]
    #[derive(Copy, Clone)]
    UnsizedToSized {
        #[ribbit(size = 32)]
        index: Option<slab::Index<size::Small>>,

        #[ribbit(size = 8)]
        class: size::Small,
    },

    #[ribbit(size = 48)]
    #[derive(Copy, Clone)]
    GlobalToLocal {
        #[ribbit(size = 32)]
        index: slab::Index<size::Small>,

        #[ribbit(size = 16)]
        version: Version,
    },

    #[ribbit(size = 48)]
    #[derive(Copy, Clone)]
    BumpToLocal {
        #[ribbit(size = 32)]
        bump: view::heap::Bump,

        #[ribbit(size = 16)]
        version: Version,
    },

    #[ribbit(size = 48)]
    #[derive(Copy, Clone)]
    LocalToGlobal {
        #[ribbit(size = 32)]
        index: slab::Index<size::Small>,

        #[ribbit(size = 16)]
        version: Version,
    },

    #[ribbit(size = 44)]
    #[derive(Copy, Clone)]
    SizedToApplication {
        #[ribbit(size = 32)]
        index: slab::Index<size::Small>,

        #[ribbit(size = 12)]
        block: Bit,
    },

    #[ribbit(size = 44)]
    #[derive(Copy, Clone)]
    ApplicationToSized {
        #[ribbit(size = 32)]
        index: slab::Index<size::Small>,

        #[ribbit(size = 12)]
        block: Bit,
    },

    #[ribbit(size = 32)]
    #[derive(Copy, Clone)]
    LocalToGlobalSave {
        #[ribbit(size = 32)]
        index: slab::Index<size::Small>,
    },

    #[ribbit(size = 60)]
    #[derive(Copy, Clone)]
    Remote {
        #[ribbit(size = 32)]
        index: slab::Index<size::Small>,

        #[ribbit(size = 12)]
        block: Bit,

        #[ribbit(size = 16)]
        version: Version,
    },
}
