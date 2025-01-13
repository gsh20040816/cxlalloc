use core::ffi;
use core::ops::Deref;
use core::ops::DerefMut;
use core::ptr;
use core::ptr::NonNull;

use crate::cas;
use crate::root;
use crate::size;
use crate::size::Bracket as _;
use crate::slab;
use crate::stat;
use crate::thread;
use crate::view;
use crate::Atomic;
use crate::Root;
use crate::BATCH_BUMP_POP;

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

impl Allocator<'_> {
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
    // pub unsafe fn realloc_untyped(
    //     &mut self,
    //     old_pointer: NonNull<ffi::c_void>,
    //     new_size: usize,
    // ) -> *mut ffi::c_void {
    //     let old_size = self.heap.class(old_pointer);
    //
    //     if old_size >= new_size {
    //         return old_pointer.as_ptr();
    //     }
    //
    //     let new_pointer = self.allocate_untyped(new_size);
    //     core::ptr::copy_nonoverlapping::<u8>(
    //         old_pointer.as_ptr().cast(),
    //         new_pointer.cast(),
    //         old_size,
    //     );
    //
    //     self.free_untyped(old_pointer);
    //     new_pointer
    // }
    //
    #[inline]
    pub unsafe fn allocate_untyped(&mut self, size: usize) -> *mut ffi::c_void {
        stat::inc(&stat::ALLOCATE);

        let class = size::Small::new(size);

        let class = match class {
            None => {
                // stat::inc(&stat::ALLOCATE_LARGE);
                // let size = size.next_multiple_of(crate::SIZE_PAGE);
                //
                // loop {
                //     match self.huge.allocate(size) {
                //         Some(descriptor) => {
                //             // save record somewhere
                //             // will it conflict with link record?
                //             //
                //             // in order to link, we need to...
                //             // - log link site
                //             // - peek allocation
                //             // - write to site
                //             // - clear allocation
                //             //
                //             // what about allocation class?
                //             // - if crash before writing to site, abort
                //             // - if crash after writing to site, recover from site
                //             //
                //             // what about huge allocation?
                //             // - need to log what
                //             // - secondary link record
                //             // - hard-code dedicated spot for huge
                //
                //             // allocate next free descriptor
                //             let class =
                //                 size::Class::new(mem::size_of::<huge::Descriptor>()).unwrap();
                //             let index = match self.owned.meta.r#sized[class].peek() {
                //                 Some(index) => index,
                //                 None => match self.allocate_small(class) {
                //                     Some(index) => index,
                //                     None => return core::ptr::null_mut(),
                //                 },
                //             };
                //
                //             let free = unsafe { &mut *self.owned.slabs[index].free.get() };
                //             let block = free.peek();
                //
                //             let offset = index.offset_block(class, block);
                //             let mut pointer =
                //                 self.heap.offset_to_pointer::<huge::Descriptor>(offset);
                //
                //             let descriptor = unsafe {
                //                 pointer.write_volatile(descriptor);
                //                 pointer.as_mut()
                //             };
                //
                //             // point at previous head in data region
                //             if let Some(prev) =
                //                 self.heap.shared.meta.huge.descriptors[self.id].load()
                //             {
                //                 let prev = self.heap.offset_to_pointer(prev);
                //                 unsafe {
                //                     crate::Box::link(&mut descriptor.next, prev.as_ref());
                //                     crate::fence();
                //                 }
                //             }
                //
                //             // update linked list of huge descriptors
                //             self.heap.shared.meta.huge.descriptors[self.id].store(Some(offset));
                //
                //             // pop block
                //             free.unset(block);
                //             if free.is_empty() {
                //                 self.detach(class);
                //             }
                //
                //             // mmap huge allocation
                //             let region = self
                //                 .heap
                //                 .shared
                //                 .backend
                //                 .allocate(
                //                     format!("huge-{}-{}", self.id, descriptor.id),
                //                     Some(
                //                         self.heap
                //                             .data
                //                             .huge()
                //                             .byte_add(descriptor.offset.into())
                //                             .cast(),
                //                     ),
                //                     descriptor.size,
                //                     0,
                //                 )
                //                 .unwrap();
                //
                //             return region.base().cast().as_ptr();
                //         }
                //         // claim a virtual address space region
                //         None => {
                //             let slot = self.heap.shared.meta.huge.claim(self.id);
                //             self.huge.claim(slot);
                //         }
                //     }
                // }
                todo!()
            }
            Some(class) => class,
        };

        stat::record_small(class);

        let id = self.id;
        let help = &self.shared.help;
        let small = &mut self.small;

        let Some(index) = small.peek(id, help, class) else {
            return ptr::null_mut();
        };

        small.pop(id, class, index)
    }

    #[inline]
    pub unsafe fn free_untyped(&mut self, pointer: NonNull<ffi::c_void>) {
        stat::inc(&stat::FREE);

        if let Some(offset) = self.huge.data.checked_pointer_to_offset(pointer) {
            stat::inc(&stat::FREE_LARGE);
            todo!();
        }

        let offset = self.heap.pointer_to_offset(pointer);
        let index = slab::Index::from(offset);

        let shared = &self.heap.shared.slabs[index];
        let owner = shared.owner.load();
        let class = owner.class();

        if owner.id() != Some(self.id) {
            return self.free_remote(offset, index, class);
        }

        stat::inc(&stat::FREE_FAST);
        let slab = &self.owned.slabs[index];
        let block = offset.index_block(class);
        let free = &mut *slab.free.get();

        self.owned
            .meta
            .log_sync(StateUnpacked::ApplicationToSized(ApplicationToSized::new(
                index, block,
            )));

        let count = free.len();
        free.set(block);

        match count {
            count if count + 1 == class.count() => {
                stat::inc(&stat::FREE_FAST_UNSIZED);
                self.owned
                    .meta
                    .sized_to_unsized(&self.owned.slabs, class, index);

                self.unsized_to_global();
            }
            0 => self.attach(class, index),
            _ => (),
        }
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
