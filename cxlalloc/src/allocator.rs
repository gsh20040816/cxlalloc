use core::ffi;
use core::marker::PhantomData;
use core::mem;
use core::num::NonZeroUsize;
use core::ptr;
use core::ptr::NonNull;

use crate::cas;
use crate::data;
use crate::huge;
use crate::recover;
use crate::recover::State;
use crate::recover::StateUnpacked;
use crate::size;
use crate::size::Bracket as _;
use crate::slab;
use crate::stat;
use crate::thread;
use crate::view;
use crate::Atomic;
use crate::Heap;
use crate::Huge;

pub struct Allocator<'raw, L: view::Lens, S: 'raw, O: 'raw> {
    pub(crate) id: L::Perspective,

    pub(crate) shared: &'raw Shared<S>,
    pub(crate) owned: L::Scope<'raw, Owned<O>>,

    pub(crate) small: Heap<'raw, L, size::Small>,
    pub(crate) huge: Huge<'raw>,
}

impl<'raw, L: view::Lens, S, O> Allocator<'raw, L, S, O> {
    pub(crate) fn new(
        id: L::Perspective,
        shared: &'raw Shared<S>,
        owned: L::Scope<'raw, Owned<O>>,
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

    pub(crate) unsafe fn focus(mut self, id: thread::Id) -> Allocator<'raw, view::Focus, S, O> {
        self.huge.focus(&self.small.data, id);

        Allocator {
            id,
            shared: self.shared,
            owned: L::focus(self.owned, id),
            small: self.small.focus(id),
            huge: self.huge,
        }
    }
}

pub(crate) struct Context<'raw> {
    pub(crate) id: thread::Id,
    pub(crate) help: &'raw cas::help::Array,
    pub(crate) log: &'raw mut Option<recover::State>,
}

#[repr(C)]
pub(crate) struct Shared<R> {
    root: Atomic<Option<data::Offset<size::Small>>>,
    _type: PhantomData<R>,
    pub(crate) help: cas::help::Array,
}

#[repr(C, align(64))]
pub(crate) struct Owned<R> {
    root: Option<data::Offset<size::Small>>,
    _type: PhantomData<R>,
    pub(crate) state: Option<recover::State>,
}

impl Context<'_> {
    #[inline]
    pub(crate) fn log(&mut self, state: StateUnpacked) {
        if !cfg!(feature = "recover-log") {
            return;
        }

        crate::fence();
        self.log_unsync(state);
        crate::fence();
    }

    #[inline]
    pub(crate) fn log_unsync(&mut self, state: StateUnpacked) {
        if !cfg!(feature = "recover-log") {
            return;
        }

        *self.log = Some(State::new(state));
        crate::flush(&self.log, false);
    }
}

impl<'raw, S, O> Allocator<'raw, view::Focus, S, O>
where
    S: 'raw,
    O: 'raw,
{
    pub fn root_shared(&self) -> Option<&'raw S> {
        let offset = self.shared.root.load()?;
        unsafe { Some(self.small.data.offset_to_pointer(offset).as_ref()) }
    }

    pub fn set_root_shared(&self, root: &'raw S) {
        let offset = self.small.data.pointer_to_offset(NonNull::from(root));
        self.shared.root.store(Some(offset));
    }

    pub fn root_owned(&self) -> Option<&'raw O> {
        let offset = self.owned.root?;
        unsafe { Some(self.small.data.offset_to_pointer(offset).as_ref()) }
    }

    pub fn root_owned_mut(&mut self) -> Option<&'raw mut O> {
        let offset = self.owned.root?;
        unsafe { Some(self.small.data.offset_to_pointer(offset).as_mut()) }
    }

    pub fn pointer_to_offset(&self, pointer: NonNull<ffi::c_void>) -> usize {
        pointer.as_ptr() as usize - self.small.data.base.as_ptr() as usize
    }

    pub fn offset_to_pointer(&self, offset: usize) -> NonNull<ffi::c_void> {
        unsafe { self.small.data.base.byte_add(offset).cast() }
    }
}

impl<S, O> Allocator<'_, view::Focus, S, O> {
    pub fn class_untyped(&self, pointer: NonNull<ffi::c_void>) -> usize {
        if let Some(offset) = self.huge.data.checked_pointer_to_offset(pointer) {
            return self.huge.class(&self.small.data, offset).get();
        }

        let offset = self.small.data.pointer_to_offset(pointer);
        self.small.class(offset).size() as usize
    }

    pub unsafe fn realloc_untyped(
        &mut self,
        old_pointer: NonNull<ffi::c_void>,
        new_size: usize,
    ) -> *mut ffi::c_void {
        let old_size = self.class_untyped(old_pointer);
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
    pub fn allocate_untyped(&mut self, size: usize) -> *mut ffi::c_void {
        stat::inc(&stat::ALLOCATE);

        let context = &mut Context {
            id: self.id,
            log: &mut self.owned.state,
            help: &self.shared.help,
        };

        let class = size::Small::new(size);
        let class = match class {
            None => {
                stat::inc(&stat::ALLOCATE_LARGE);
                let size = NonZeroUsize::new(size.next_multiple_of(crate::SIZE_PAGE)).unwrap();

                let class = size::Small::new(mem::size_of::<huge::Descriptor>()).unwrap();
                let index = self.small.peek(context, class).unwrap();
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
                let allocation = self.huge.allocate(context.id, data, size, descriptor);
                // FIXME: pop before mmap in `self.huge.allocate` or check if
                // allocated on recovery
                self.small.pop(context, class, index);
                return allocation;
            }
            Some(class) => class,
        };

        stat::record_small(class);

        let Some(index) = self.small.peek(context, class) else {
            return ptr::null_mut();
        };

        self.small.pop(context, class, index)
    }

    #[inline]
    pub unsafe fn free_untyped(&mut self, pointer: NonNull<ffi::c_void>) {
        stat::inc(&stat::FREE);

        if let Some(offset) = self.huge.data.checked_pointer_to_offset(pointer) {
            stat::inc(&stat::FREE_LARGE);
            return self.huge.free(&self.small.data, offset);
        }

        let offset = self.small.data.pointer_to_offset(pointer);
        let context = &mut Context {
            id: self.id,
            help: &self.shared.help,
            log: &mut self.owned.state,
        };

        self.small.free(context, offset)
    }
}

#[derive(Copy, Clone)]
#[ribbit::pack(size = 32, nonzero)]
pub(crate) enum Index {
    #[ribbit(size = 32, nonzero)]
    Small(slab::Index<size::Small>),
}

impl From<slab::Index<size::Small>> for Index {
    fn from(index: slab::Index<size::Small>) -> Self {
        Self::new(IndexUnpacked::Small(index))
    }
}

#[derive(Copy, Clone)]
#[ribbit::pack(size = 8)]
pub(crate) enum Bracket {
    #[ribbit(size = 8)]
    Small(size::Small),
}

impl From<size::Small> for Bracket {
    fn from(class: size::Small) -> Self {
        Self::new(BracketUnpacked::Small(class))
    }
}
