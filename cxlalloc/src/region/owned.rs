use core::cell::UnsafeCell;

use crate::atomic::Version;
use crate::bitset::Bit;
use crate::crash;
use crate::size;
use crate::slab;
use crate::thread;

use super::shared::Bump;

#[repr(C, align(64))]
pub(crate) struct Owned(thread::Array<UnsafeCell<Free>>);

pub(crate) struct Free {
    pub(crate) r#unsized: slab::LocalStack,
    pub(crate) r#sized: size::Array<slab::LocalStack>,
}

impl Free {
    // #[inline]
    // pub(crate) fn log_sync(&mut self, state: StateUnpacked) {
    //     if !cfg!(feature = "recover-log") {
    //         return;
    //     }
    //
    //     crate::fence();
    //     self.log_unsync(State::new(state));
    //     crate::fence();
    // }
    //
    // #[inline]
    // pub(crate) fn log_unsync(&mut self, state: State) {
    //     if !cfg!(feature = "recover-log") {
    //         return;
    //     }
    //
    //     self.state.store(Some(state));
    //     crate::flush(&self.state, false);
    // }

    pub(crate) fn unsized_to_sized(
        &mut self,
        owned: &slab::Slice<slab::Owned>,
        shared: &slab::Slice<slab::Shared>,
        id: thread::Id,
        class: size::Class,
    ) -> bool {
        let Some(index) = self.r#unsized.peek() else {
            return false;
        };

        crash::define!(unsized_to_sized_pre_log);

        let slab = &owned[index];
        let next = slab.next.load();

        // self.log_sync(StateUnpacked::UnsizedToSized(UnsizedToSized::new(
        //     next, class,
        // )));

        self.r#sized[class].push(owned, index);
        unsafe {
            (*slab.free.get()).fill(class.count());
        }

        shared[index]
            .owner
            .store(slab::shared::Owner::new(class, Some(id)));
        crate::flush(&shared[index].owner, false);

        let count = self.r#unsized.len();
        self.r#unsized.set(next, count - 1);
        true
    }

    #[cold]
    pub(crate) fn sized_to_unsized(
        &mut self,
        slabs: &slab::Slice<slab::Owned>,
        class: size::Class,
        index: slab::Index,
    ) {
        // Special case: not in sized list
        if class == size::SLAB {
            return self.r#unsized.push(slabs, index);
        }

        let next = slabs[index].next.load();

        let mut walk = self.r#sized[class].peek().unwrap();

        if walk == index {
            let count = self.r#sized[class].len();
            self.r#sized[class].set(next, count - 1);
        } else {
            let prev = loop {
                match slabs[walk].next.load() {
                    None => panic!("removing non-existent slab {} {}", index, class),
                    Some(next) if next == index => break walk,
                    Some(next) => walk = next,
                }
            };

            slabs[prev].next.store(next);
            crate::flush(&slabs[prev], false);
        };

        self.r#unsized.push(slabs, index);
    }
}

#[ribbit::pack(size = 64, nonzero)]
#[derive(Copy, Clone)]
pub(crate) enum State {
    #[ribbit(size = 40)]
    #[derive(Copy, Clone)]
    UnsizedToSized {
        #[ribbit(size = 32)]
        index: Option<slab::Index>,

        #[ribbit(size = 8)]
        class: size::Class,
    },

    #[ribbit(size = 48)]
    #[derive(Copy, Clone)]
    GlobalToLocal {
        #[ribbit(size = 32)]
        index: slab::Index,

        #[ribbit(size = 16)]
        version: Version,
    },

    #[ribbit(size = 48)]
    #[derive(Copy, Clone)]
    BumpToLocal {
        #[ribbit(size = 32)]
        bump: Bump,

        #[ribbit(size = 16)]
        version: Version,
    },

    #[ribbit(size = 48)]
    #[derive(Copy, Clone)]
    LocalToGlobal {
        #[ribbit(size = 32)]
        index: slab::Index,

        #[ribbit(size = 16)]
        version: Version,
    },

    #[ribbit(size = 44)]
    #[derive(Copy, Clone)]
    SizedToApplication {
        #[ribbit(size = 32)]
        index: slab::Index,

        #[ribbit(size = 12)]
        block: Bit,
    },

    #[ribbit(size = 44)]
    #[derive(Copy, Clone)]
    ApplicationToSized {
        #[ribbit(size = 32)]
        index: slab::Index,

        #[ribbit(size = 12)]
        block: Bit,
    },

    #[ribbit(size = 32)]
    #[derive(Copy, Clone)]
    LocalToGlobalSave {
        #[ribbit(size = 32)]
        index: slab::Index,
    },

    #[ribbit(size = 60)]
    #[derive(Copy, Clone)]
    Remote {
        #[ribbit(size = 32)]
        index: slab::Index,

        #[ribbit(size = 12)]
        block: Bit,

        #[ribbit(size = 16)]
        version: Version,
    },
}
