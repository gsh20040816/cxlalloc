use core::ffi;
use core::mem;
use core::ptr;
use core::ptr::NonNull;

use crate::cas;
use crate::data;
use crate::huge;
use crate::log;
use crate::size;
use crate::size::Bracket as _;
use crate::slab;
use crate::stat;
use crate::thread;
use crate::view;
use crate::Heap;
use crate::Huge;

pub struct Allocator<'raw, L: view::Lens> {
    pub(crate) id: thread::Id,

    pub(crate) shared: &'raw Shared,
    pub(crate) owned: L::Scope<'raw, Owned>,

    pub(crate) small: Heap<'raw, L, size::Small>,
    pub(crate) huge: Huge<'raw>,
}

impl<'raw, L: view::Lens> Allocator<'raw, L> {
    pub(crate) fn new(
        id: thread::Id,
        shared: &'raw Shared,
        owned: L::Scope<'raw, Owned>,
        small: Heap<'raw, L, size::Small>,
        huge: Huge<'raw>,
    ) -> Self {
        Self {
            id,
            shared,
            owned,
            small,
            huge,
        }
    }

    pub(crate) unsafe fn focus(self, id: thread::Id) -> Allocator<'raw, view::Focus> {
        Allocator {
            id,
            shared: self.shared,
            owned: L::focus(self.owned, id),
            small: self.small.focus(id),
            huge: self.huge,
        }
    }
}

#[repr(C)]
pub(crate) struct Shared {
    // pub(crate) root: Atomic<Option<data::Offset>>,
    pub(crate) help: cas::help::Array,
}

#[repr(C, align(64))]
pub(crate) struct Owned {
    // pub(crate) root: Option<data::Offset>,
    pub(crate) state: log::State,
}

impl Allocator<'_, view::Focus> {
    pub fn class(&self, pointer: NonNull<ffi::c_void>) -> usize {
        if let Some(offset) = self.huge.data.checked_pointer_to_offset(pointer) {
            todo!();
        }

        let offset = self.small.data.pointer_to_offset(pointer);
        self.small.class(offset).size() as usize
    }

    // pub unsafe fn root_untyped(&self, root: root::Index) -> Option<NonNull<ffi::c_void>> {
    //     let offset = self.heap.shared[root].load()?;
    //     // HACK: support flag-guarded initialization of large allocations
    //     self.heap().replay_log(false);
    //     Some(self.heap.offset_to_pointer(offset))
    // }
    //
    // pub unsafe fn set_root_untyped(
    //     &self,
    //     root: root::Index,
    //     pointer: Option<NonNull<ffi::c_void>>,
    // ) {
    //     let offset = pointer.map(|pointer| self.heap.pointer_to_offset(pointer));
    //     self.heap.shared[root].store(offset);
    // }
    //
    pub unsafe fn realloc_untyped(
        &mut self,
        old_pointer: NonNull<ffi::c_void>,
        new_size: usize,
    ) -> *mut ffi::c_void {
        if let Some(offset) = self.huge.data.checked_pointer_to_offset(old_pointer) {
            todo!();
        }

        let old_offset = self.small.data.pointer_to_offset(old_pointer);
        let old_size = self.small.class(old_offset).size() as usize;

        if old_size >= new_size {
            return old_pointer.as_ptr();
        }

        let new_pointer = self.allocate_untyped(new_size);
        core::ptr::copy_nonoverlapping::<u8>(
            old_pointer.as_ptr().cast(),
            new_pointer.cast(),
            old_size,
        );

        self.free_untyped(old_pointer);
        new_pointer
    }

    #[inline]
    pub unsafe fn allocate_untyped(&mut self, size: usize) -> *mut ffi::c_void {
        stat::inc(&stat::ALLOCATE);

        let id = self.id;
        let help = &self.shared.help;
        let class = size::Small::new(size);

        let class = match class {
            None => {
                stat::inc(&stat::ALLOCATE_LARGE);
                let size = size.next_multiple_of(crate::SIZE_PAGE);

                let class = size::Small::new(mem::size_of::<huge::Descriptor>()).unwrap();
                let index = self.small.peek(id, help, class).unwrap();
                let free = unsafe { &mut *self.small.slabs[index].local.free.get() };
                let block = free.peek();

                let offset = data::Offset::from_block(index, class, block);
                let descriptor = unsafe {
                    self.small
                        .data
                        .offset_to_pointer::<huge::Descriptor>(offset)
                        .as_mut()
                };

                let data = &self.small.data;
                let allocation = self.huge.allocate(id, data, size, descriptor);
                // FIXME: pop before mmap in `self.huge.allocate` or check if
                // allocated on recovery
                self.small.pop(id, class, index);
                return allocation;
            }
            Some(class) => class,
        };

        stat::record_small(class);

        let Some(index) = self.small.peek(id, help, class) else {
            return ptr::null_mut();
        };

        self.small.pop(id, class, index)
    }

    #[inline]
    pub unsafe fn free_untyped(&mut self, pointer: NonNull<ffi::c_void>) {
        stat::inc(&stat::FREE);

        if let Some(offset) = self.huge.data.checked_pointer_to_offset(pointer) {
            stat::inc(&stat::FREE_LARGE);
            return self.huge.free(&self.small.data, offset);
        }

        let offset = self.small.data.pointer_to_offset(pointer);
        let id = self.id;
        let help = &self.shared.help;
        self.small.free(id, help, offset)
    }
}

#[cfg(feature = "extend")]
impl Allocator<'_, view::Focus> {
    pub fn extend(&mut self) {
        todo!()
    }

    pub fn epoch(&self) -> crate::extend::Epoch {
        todo!()
    }
}

#[derive(Copy, Clone)]
#[ribbit::pack(size = 32, nonzero)]
pub(crate) enum Index {
    #[ribbit(size = 32, nonzero)]
    Small(slab::Index<size::Small>),
}

#[derive(Copy, Clone)]
#[ribbit::pack(size = 8)]
pub(crate) enum Class {
    #[ribbit(size = 8)]
    Small(size::Small),
}
