use core::ffi;
use core::marker::PhantomData;
use core::mem;
use core::mem::MaybeUninit;
use core::num::NonZeroUsize;
use core::ptr;
use core::ptr::NonNull;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::Ordering;

use ribbit::atomic::Atomic64;
use ribbit::unpack;

use crate::cache;
use crate::cas;
use crate::data;
use crate::huge;
use crate::recover;
use crate::recover::State;
use crate::size;
use crate::size::Bracket as _;
use crate::slab;
use crate::stat;
use crate::thread;
use crate::view;
use crate::Data;
use crate::Heap;
use crate::Huge;

pub struct Allocator<'raw, L: view::Lens, S: 'raw, O: 'raw> {
    pub(crate) id: L::Perspective,

    pub(crate) shared: &'raw Shared<S>,
    pub(crate) owned: L::Scope<'raw, Owned>,

    pub(crate) small: Heap<'raw, L, size::Small>,
    pub(crate) large: Heap<'raw, L, size::Large>,
    pub(crate) huge: Huge<'raw>,

    _owned: PhantomData<O>,
}

impl<'raw, L: view::Lens, S, O> Allocator<'raw, L, S, O> {
    pub(crate) fn new(
        id: L::Perspective,
        shared: &'raw Shared<S>,
        owned: L::Scope<'raw, Owned>,
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
            _owned: PhantomData,
        }
    }

    pub(crate) unsafe fn focus(mut self, id: thread::Id) -> Allocator<'raw, view::Focus, S, O> {
        self.huge.focus(&self.small.data, id);

        crate::mcas::THREAD_ID.with(|save| save.store(u16::from(id), Ordering::Relaxed));

        let mut allocator = Allocator {
            id,
            shared: self.shared,
            owned: L::focus(self.owned, id),
            small: self.small.focus(id),
            large: self.large.focus(id),
            huge: self.huge,
            _owned: PhantomData,
        };

        allocator.recover();
        allocator
    }
}

pub(crate) struct Context<'raw> {
    pub(crate) id: thread::Id,
    pub(crate) help: &'raw cas::help::Array,
    pub(crate) owned: &'raw Owned,
}

#[repr(C)]
pub(crate) struct Shared<R> {
    root: Atomic64<Option<data::Offset<size::Small>>>,
    _root: PhantomData<R>,

    /// Untyped roots
    /// Memento uses 512+ :(
    roots: [Atomic64<Option<data::Offset<size::Small>>>; 1024],

    pub(crate) help: cas::help::Array,
}

#[repr(C, align(64))]
pub(crate) struct Owned {
    root: AtomicU64,

    pub(crate) link: Atomic64<Option<data::Offset<size::Small>>>,
    pub(crate) free: Atomic64<Option<data::Offset<size::Small>>>,
    pub(crate) state: Atomic64<Option<recover::State>>,
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

        self.owned
            .state
            .store(Some(state.into()), Ordering::Relaxed);
        cache::flush(&self.owned.state, cache::Invalidate::No);
    }
}

/// Type-safe API
impl<'raw, S, O> Allocator<'raw, view::Focus, S, O>
where
    S: 'raw,
    O: 'raw,
{
    pub fn report(&self) -> impl Iterator<Item = stat::Report> + '_ {
        self.small
            .report(self.id)
            .chain(self.large.report(self.id))
            .chain(self.huge.report(self.id))
    }

    pub fn root_shared(&self, ordering: Ordering) -> Option<&'raw S> {
        let offset = self.shared.root.load(ordering)?;
        unsafe { Some(self.small.data.offset_to_pointer(offset).as_ref()) }
    }

    pub fn set_root_shared(&self, root: &'raw S, ordering: Ordering) {
        let offset = self
            .small
            .data
            .pointer_to_offset(NonNull::from(root))
            .unwrap();
        self.shared.root.store(Some(offset), ordering);
    }

    pub fn root_owned(&self) -> Option<&'raw O> {
        let offset = self.owned.root.load(Ordering::Relaxed);
        let offset = self.small.data.offset_to_offset(offset as usize);
        unsafe { Some(self.small.data.offset_to_pointer(offset).as_ref()) }
    }

    pub fn root_owned_mut(&mut self) -> Option<&'raw mut O> {
        let offset = self.owned.root.load(Ordering::Relaxed);
        let offset = self.small.data.offset_to_offset(offset as usize);
        unsafe { Some(self.small.data.offset_to_pointer(offset).as_mut()) }
    }

    pub fn pointer_to_offset(&self, pointer: NonNull<ffi::c_void>) -> usize {
        pointer.as_ptr() as usize - self.small.data.base.as_ptr() as usize
    }

    pub fn offset_to_pointer(&self, offset: usize) -> NonNull<ffi::c_void> {
        unsafe { self.small.data.base.byte_add(offset).cast() }
    }

    pub fn allocate<'link, T: Default>(
        &mut self,
        link: Option<&'link mut Option<crate::Box<T>>>,
    ) -> &'link mut T {
        self.allocate_checked(link).unwrap()
    }

    pub fn allocate_checked<'link, T: Default>(
        &mut self,
        link: Option<&'link mut Option<crate::Box<T>>>,
    ) -> Option<&'link mut T> {
        let size = mem::size_of::<T>();

        let Some(class) = size::Small::new(size) else {
            return self.allocate_large_checked(size, link);
        };

        let context = &mut Context {
            id: self.id,
            help: &self.shared.help,
            owned: self.owned,
        };

        Self::allocate_heap(
            context,
            self.small.data.clone(),
            &mut self.small,
            class,
            link,
        )
    }

    fn allocate_large_checked<'link, T: Default>(
        &mut self,
        size: usize,
        link: Option<&'link mut Option<crate::Box<T>>>,
    ) -> Option<&'link mut T> {
        let Some(class) = size::Large::new(size) else {
            todo!()
        };

        let context = &mut Context {
            id: self.id,
            help: &self.shared.help,
            owned: self.owned,
        };

        Self::allocate_heap(
            context,
            self.small.data.clone(),
            &mut self.large,
            class,
            link,
        )
    }

    fn allocate_heap<'link, B: size::Bracket, T>(
        context: &mut Context,
        base: Data<size::Small>,
        heap: &mut Heap<'raw, view::Focus, B>,
        class: B,
        mut link: Option<&'link mut Option<crate::Box<T>>>,
    ) -> Option<&'link mut T>
    where
        T: Default,
        slab::Local<B>: slab::local::Cache<B>,
        recover::State: From<recover::HeapState<B>>,
    {
        let (index, block) = heap.peek(context, class)?;

        #[cfg(feature = "recover-log")]
        match link.as_mut() {
            None => context.owned.link.store(None, Ordering::Relaxed),
            Some(link) => {
                let offset = base.pointer_to_offset(NonNull::from(link)).unwrap();
                context.owned.link.store(Some(offset), Ordering::Relaxed);
            }
        }

        cache::flush(&context.owned.link, cache::Invalidate::No);
        cache::fence();

        context.log(recover::State::from(recover::HeapState::new(
            <unpack![recover::HeapState::<B>]>::SizedToApplication(
                recover::SizedToApplication::new(index, block),
            ),
        )));

        let offset = data::Offset::from_block(class, index, block);
        let allocation = unsafe {
            let uninit = heap
                .data
                .offset_to_pointer::<MaybeUninit<T>>(offset)
                .as_mut();

            uninit.as_mut_ptr().write(T::default());
            uninit.assume_init_mut()
        };

        cache::flush(allocation, cache::Invalidate::No);
        cache::fence();

        #[cfg(feature = "recover-log")]
        match link.as_mut() {
            None => {
                context
                    .owned
                    .root
                    .store(u64::from(offset), Ordering::Relaxed);
                cache::flush(&context.owned.root, cache::Invalidate::No);
            }
            Some(link) => {
                crate::Box::link(link, Some(allocation));
                cache::flush(link, cache::Invalidate::No);
            }
        }

        heap.pop(context, class, index, block);

        Some(allocation)
    }

    pub fn free<'link, T: Default>(&mut self, link: Option<&'link mut Option<crate::Box<T>>>) {
        let size = mem::size_of::<T>();

        let Some(_) = size::Small::new(size) else {
            return self.free_large_checked(size, link);
        };

        let context = &mut Context {
            id: self.id,
            help: &self.shared.help,
            owned: self.owned,
        };

        Self::free_heap(context, self.small.data.clone(), &mut self.small, link)
    }

    fn free_large_checked<'link, T: Default>(
        &mut self,
        size: usize,
        link: Option<&'link mut Option<crate::Box<T>>>,
    ) {
        let Some(_) = size::Large::new(size) else {
            todo!()
        };

        let context = &mut Context {
            id: self.id,
            help: &self.shared.help,
            owned: self.owned,
        };

        Self::free_heap(context, self.small.data.clone(), &mut self.large, link)
    }

    fn free_heap<B: size::Bracket, T>(
        context: &mut Context,
        base: Data<size::Small>,
        heap: &mut Heap<'raw, view::Focus, B>,
        mut link: Option<&mut Option<crate::Box<T>>>,
    ) where
        slab::Local<B>: slab::local::Cache<B>,
        recover::State: From<recover::HeapState<B>>,
    {
        let offset = match link.as_mut() {
            None => {
                #[cfg(feature = "recover-log")]
                context.owned.free.store(None, Ordering::Relaxed);

                let offset = context.owned.root.load(Ordering::Relaxed);
                heap.data.offset_to_offset(offset as usize)
            }
            Some(link) => {
                #[cfg(feature = "recover-log")]
                {
                    let offset = base.pointer_to_offset(NonNull::from(&mut **link)).unwrap();
                    context.owned.free.store(Some(offset), Ordering::Relaxed);
                }

                let offset = heap
                    .checked_pointer_to_offset(NonNull::from(link.as_mut().unwrap()).cast())
                    .unwrap();
                offset
            }
        };

        heap.free(context, offset);

        match link {
            None => context.owned.root.store(0, Ordering::Relaxed),
            Some(link) => {
                *link = None;
            }
        }

        context.owned.state.store(None, Ordering::Relaxed);
    }
}

/// Untyped API
impl<S, O> Allocator<'_, view::Focus, S, O> {
    pub fn root_untyped(&self, index: usize) -> Option<NonNull<ffi::c_void>> {
        let offset = self.shared.roots[index].load(Ordering::Acquire)?;
        let pointer = self.small.data.offset_to_pointer(offset);
        log::trace!("get root {} {:?} {:#x?}", index, offset, pointer);
        Some(pointer)
    }

    pub fn set_root_untyped(&self, index: usize, pointer: *mut ffi::c_void) {
        let offset =
            NonNull::new(pointer).and_then(|pointer| self.small.data.pointer_to_offset(pointer));
        log::trace!("set root {} {:?} {:#x?}", index, offset, pointer);
        self.shared.roots[index].store(offset, Ordering::Release);
    }

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
            help: &self.shared.help,
            owned: self.owned,
        };

        let Some((index, block)) = self.small.peek(context, class) else {
            return ptr::null_mut();
        };

        let p = self.small.pop(context, class, index, block);

        // FIXME: use transactional allocation in state machine test
        // since it tries to recover after every allocation
        self.owned.state.store(None, Ordering::Relaxed);

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
            owned: self.owned,
        };

        self.small.free(context, offset);
        self.owned.state.store(None, Ordering::Relaxed);
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
            help: &self.shared.help,
            owned: self.owned,
        };

        let Some((index, block)) = self.large.peek(context, class) else {
            return ptr::null_mut();
        };

        let p = self.large.pop(context, class, index, block);
        log::trace!("allocate large {:#x} {:#x?}", size, p);
        p
    }

    #[cold]
    fn allocate_huge(&mut self, size: usize) -> *mut ffi::c_void {
        let context = &mut Context {
            id: self.id,
            help: &self.shared.help,
            owned: self.owned,
        };

        let size = NonZeroUsize::new(size.next_multiple_of(crate::SIZE_PAGE)).unwrap();

        // FIXME: reuse API shouldn't be exposed here
        let (offset, index) = match self.huge.reuse(&self.small.data, self.id) {
            Some(offset) => (offset, None),
            None => {
                let (index, block) = self.small.peek(context, huge::Descriptor::CLASS).unwrap();
                (
                    data::Offset::from_block(huge::Descriptor::CLASS, index, block),
                    Some((index, block)),
                )
            }
        };

        let descriptor = unsafe { &mut *self.small.data.offset_to_pointer(offset).as_ptr() };
        let allocation = self.huge.allocate(
            context.id,
            &self.small.data,
            size,
            descriptor,
            index.is_none(),
        );

        // FIXME: pop before mmap in `self.huge.allocate` or check if
        // allocated on recovery
        log::trace!("allocate huge {:#x} {:#x?}", size, allocation);
        if let Some((index, block)) = index {
            self.small
                .pop(context, huge::Descriptor::CLASS, index, block);
        }

        allocation
    }

    #[cold]
    fn free_large(&mut self, pointer: NonNull<ffi::c_void>) {
        let Some(offset) = self.large.checked_pointer_to_offset(pointer) else {
            return self.free_huge(pointer);
        };

        let context = &mut Context {
            id: self.id,
            help: &self.shared.help,
            owned: self.owned,
        };

        self.large.free(context, offset)
    }

    #[cold]
    fn free_huge(&mut self, pointer: NonNull<ffi::c_void>) {
        if let Some(offset) = self.huge.checked_pointer_to_offset(pointer) {
            let context = &mut Context {
                id: self.id,
                help: &self.shared.help,
                owned: self.owned,
            };

            self.huge.free(context, &self.small.data, offset);
        }
    }
}
