use core::ffi;
use core::marker::PhantomData;
use core::mem;
use core::num::NonZeroUsize;
use core::ptr;
use core::ptr::NonNull;

use crate::cache;
use crate::cas;
use crate::data;
use crate::huge;
use crate::recover;
use crate::recover::State;
use crate::size;
use crate::size::Bracket as _;
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
    pub(crate) large: Heap<'raw, L, size::Large>,
    pub(crate) huge: Huge<'raw>,
}

impl<'raw, L: view::Lens, S, O> Allocator<'raw, L, S, O> {
    pub(crate) fn new(
        id: L::Perspective,
        shared: &'raw Shared<S>,
        owned: L::Scope<'raw, Owned<O>>,
        small: Heap<'raw, L, size::Small>,
        large: Heap<'raw, L, size::Large>,
        huge: Huge<'raw>,
    ) -> Self {
        Self {
            id,
            shared,
            owned,
            small,
            large,
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
            large: self.large.focus(id),
            huge: self.huge,
        }
    }

    pub fn report_process(&self) -> impl Iterator<Item = stat::EventReport> + '_ {
        self.small
            .report(None)
            .chain(self.large.report(None))
            .chain(self.huge.report(None))
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
    _root: PhantomData<R>,

    /// Untyped roots
    /// Memento uses 512+ :(
    roots: [Atomic<Option<data::Offset<size::Small>>>; 1024],

    pub(crate) help: cas::help::Array,
}

#[repr(C, align(64))]
pub(crate) struct Owned<R> {
    root: Option<data::Offset<size::Small>>,
    _root: PhantomData<R>,
    pub(crate) state: Option<recover::State>,
}

impl Context<'_> {
    #[inline]
    pub(crate) fn log<S: Into<State>>(&mut self, state: S) {
        if !cfg!(feature = "recover-log") {
            return;
        }

        cache::fence();
        self.log_unsync(state);
        cache::fence();
    }

    #[inline]
    pub(crate) fn log_unsync<S: Into<State>>(&mut self, state: S) {
        if !cfg!(feature = "recover-log") {
            return;
        }

        *self.log = Some(state.into());
        cache::flush(&self.log, cache::Invalidate::No);
    }
}

impl<'raw, S, O> Allocator<'raw, view::Focus, S, O>
where
    S: 'raw,
    O: 'raw,
{
    pub fn report_thread(&self) -> impl Iterator<Item = stat::EventReport> + '_ {
        let id = Some(self.id);
        self.small
            .report(id)
            .chain(self.large.report(id))
            .chain(self.huge.report(id))
    }

    pub fn root_shared(&self) -> Option<&'raw S> {
        let offset = self.shared.root.load()?;
        unsafe { Some(self.small.data.offset_to_pointer(offset).as_ref()) }
    }

    pub fn set_root_shared(&self, root: &'raw S) {
        let offset = self
            .small
            .data
            .pointer_to_offset(NonNull::from(root))
            .unwrap();
        self.shared.root.store(Some(offset));
    }

    pub fn root_untyped(&self, index: usize) -> Option<NonNull<ffi::c_void>> {
        let offset = self.shared.roots[index].load()?;
        let pointer = self.small.data.offset_to_pointer(offset);
        log::trace!("get root {} {:?} {:#x?}", index, offset, pointer);
        Some(pointer)
    }

    pub fn set_root_untyped(&self, index: usize, pointer: *mut ffi::c_void) {
        let offset =
            NonNull::new(pointer).and_then(|pointer| self.small.data.pointer_to_offset(pointer));
        log::trace!("set root {} {:?} {:#x?}", index, offset, pointer);
        self.shared.roots[index].store(offset);
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
        if let Some(offset) = self.small.checked_pointer_to_offset(pointer) {
            return self.small.class(offset).size() as usize;
        }

        if let Some(offset) = self.large.checked_pointer_to_offset(pointer) {
            return self.large.class(offset).size() as usize;
        }

        if let Some(offset) = self.huge.checked_pointer_to_offset(pointer) {
            return self.huge.class(&self.small.data, offset).get();
        }

        panic!("Unrecognized pointer: {:#x?}", pointer)
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
        let Some(class) = size::Small::new(size) else {
            return self.allocate_large(size);
        };

        let context = &mut Context {
            id: self.id,
            log: &mut self.owned.state,
            help: &self.shared.help,
        };

        let Some(index) = self.small.peek(context, class) else {
            return ptr::null_mut();
        };

        let p = self.small.pop(context, class, index);
        log::trace!("allocate small {:#x} {:#x?}", size, p);
        p
    }

    #[inline]
    pub fn free_untyped(&mut self, pointer: NonNull<ffi::c_void>) {
        let Some(offset) = self.small.checked_pointer_to_offset(pointer) else {
            return self.free_large(pointer);
        };

        let context = &mut Context {
            id: self.id,
            help: &self.shared.help,
            log: &mut self.owned.state,
        };

        self.small.free(context, offset);
    }
}

impl<S, O> Allocator<'_, view::Focus, S, O> {
    #[cold]
    fn allocate_large(&mut self, size: usize) -> *mut ffi::c_void {
        let Some(class) = size::Large::new(size) else {
            return self.allocate_huge(size);
        };

        let context = &mut Context {
            id: self.id,
            log: &mut self.owned.state,
            help: &self.shared.help,
        };

        let Some(index) = self.large.peek(context, class) else {
            return ptr::null_mut();
        };

        let p = self.large.pop(context, class, index);
        log::trace!("allocate large {:#x} {:#x?}", size, p);
        p
    }

    #[cold]
    fn allocate_huge(&mut self, size: usize) -> *mut ffi::c_void {
        let context = &mut Context {
            id: self.id,
            log: &mut self.owned.state,
            help: &self.shared.help,
        };

        let size = NonZeroUsize::new(size.next_multiple_of(crate::SIZE_PAGE)).unwrap();
        let class = size::Small::new(mem::size_of::<huge::Descriptor>()).unwrap();

        let index = self.small.peek(context, class).unwrap();
        let free = unsafe { &mut *self.small.slabs.local(index).free.get() };
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
        log::trace!("allocate huge {:#x} {:#x?}", size, allocation);
        self.small.pop(context, class, index);
        allocation
    }

    #[cold]
    fn free_large(&mut self, pointer: NonNull<ffi::c_void>) {
        let Some(offset) = self.large.checked_pointer_to_offset(pointer) else {
            return self.free_huge(pointer);
        };

        let context = &mut Context {
            id: self.id,
            log: &mut self.owned.state,
            help: &self.shared.help,
        };

        self.large.free(context, offset)
    }

    #[cold]
    fn free_huge(&mut self, pointer: NonNull<ffi::c_void>) {
        if let Some(offset) = self.huge.checked_pointer_to_offset(pointer) {
            let context = &mut Context {
                id: self.id,
                log: &mut self.owned.state,
                help: &self.shared.help,
            };

            self.huge.free(context, &self.small.data, offset);
        }
    }
}
