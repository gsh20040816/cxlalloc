use core::ffi;
use core::mem;
use core::ops::Deref;
use core::ops::DerefMut;
use core::ptr;
use core::ptr::NonNull;

use crate::data;
use crate::huge;
use crate::size;
use crate::size::Bracket as _;
use crate::slab;
use crate::stat;
use crate::view;

pub struct Allocator<'raw>(view::Allocator<'raw, view::Focus>);

impl<'raw> Deref for Allocator<'raw> {
    type Target = view::Allocator<'raw, view::Focus>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Allocator<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<'raw> Allocator<'raw> {
    pub(crate) fn new(inner: view::Allocator<'raw, view::Focus>) -> Self {
        Self(inner)
    }
}

impl<'raw> From<view::Allocator<'raw, view::Focus>> for Allocator<'raw> {
    fn from(inner: view::Allocator<'raw, view::Focus>) -> Self {
        Self(inner)
    }
}

impl Allocator<'_> {
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

                loop {
                    match self.huge.allocate(size) {
                        Some(descriptor) => {
                            // save record somewhere
                            // will it conflict with link record?
                            //
                            // in order to link, we need to...
                            // - log link site
                            // - peek allocation
                            // - write to site
                            // - clear allocation
                            //
                            // what about allocation class?
                            // - if crash before writing to site, abort
                            // - if crash after writing to site, recover from site
                            //
                            // what about huge allocation?
                            // - need to log what
                            // - secondary link record
                            // - hard-code dedicated spot for huge

                            // allocate next free descriptor
                            let class =
                                size::Small::new(mem::size_of::<huge::Descriptor>()).unwrap();

                            let index = self.small.peek(id, help, class).unwrap();
                            let free = unsafe { &mut *self.small.slabs[index].local.free.get() };
                            let block = free.peek();

                            let offset = data::Offset::from_block(index, class, block);
                            let mut pointer = self
                                .small
                                .data
                                .offset_to_pointer::<huge::Descriptor>(offset);

                            let descriptor = unsafe {
                                pointer.write_volatile(descriptor);
                                pointer.as_mut()
                            };

                            // point at previous head in data region
                            if let Some(prev) = self.huge.get(self.id, &self.small.data) {
                                unsafe {
                                    crate::Box::link(&mut descriptor.next, prev);
                                    crate::fence();
                                }
                            }

                            // update linked list of huge descriptors
                            self.huge.set(id, &self.small.data, descriptor);

                            // pop block
                            self.small.pop(id, class, index);

                            // mmap huge allocation
                            let region = self
                                .huge
                                .backend
                                .allocate(
                                    format!("huge-{}-{}", self.id, descriptor.id),
                                    Some(self.huge.data.offset_to_pointer(descriptor.offset)),
                                    descriptor.size,
                                    None,
                                )
                                .unwrap();

                            return region.base().cast().as_ptr();
                        }
                        // claim a virtual address space region
                        None => self.huge.claim(id),
                    }
                }
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
impl Allocator<'_> {
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
