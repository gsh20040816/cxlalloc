use core::marker::PhantomData;
use core::num::NonZeroU16;
use core::ops::Index;
use core::ops::IndexMut;
use core::ptr::NonNull;

use crate::atomic::NonZero;
use crate::atomic::Packed;
use crate::raw;
use crate::COUNT_THREAD;

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct Id(NonZeroU16);

impl Id {
    pub const unsafe fn new(id: u16) -> Self {
        assert!(id < u16::MAX);
        Self(NonZeroU16::new_unchecked(id.wrapping_add(1)))
    }
}

unsafe impl Packed for Id {
    const BITS: u8 = 16;

    fn pack(&self) -> u64 {
        self.0.get() as u64
    }

    fn unpack(value: u64) -> Self {
        unsafe { Id(NonZeroU16::new_unchecked(value as u16)) }
    }
}

unsafe impl NonZero for Id {}

#[repr(C)]
pub struct Array<T>([T; COUNT_THREAD]);

impl<T> Index<&Id> for Array<T> {
    type Output = T;
    fn index(&self, index: &Id) -> &Self::Output {
        &self.0[index.0.get() as usize]
    }
}

pub(crate) struct Slice<'raw, T> {
    base: NonNull<T>,
    _raw: PhantomData<&'raw raw::Region>,
}

impl<T> Slice<'_, T> {
    pub(crate) unsafe fn from_raw(region: &raw::Region, offset: usize) -> Self {
        let base = region.base().byte_add(offset).as_ptr().cast::<T>();

        Self {
            base: NonNull::new(base).unwrap(),
            _raw: PhantomData,
        }
    }
}

impl<T> Index<&Id> for Slice<'_, T> {
    type Output = T;
    fn index(&self, index: &Id) -> &Self::Output {
        unsafe { self.base.add(index.0.get() as usize).as_ref() }
    }
}

impl<T> Index<&mut Id> for Slice<'_, T> {
    type Output = T;
    fn index(&self, index: &mut Id) -> &Self::Output {
        unsafe { self.base.add(index.0.get() as usize).as_ref() }
    }
}

impl<T> IndexMut<&mut Id> for Slice<'_, T> {
    fn index_mut(&mut self, index: &mut Id) -> &mut Self::Output {
        unsafe { self.base.add(index.0.get() as usize).as_mut() }
    }
}
