use core::marker::PhantomData;
use core::num::NonZeroIsize;
use core::ops::Deref;
use core::ops::DerefMut;

pub struct Box<T> {
    delta: NonZeroIsize,
    _type: PhantomData<T>,
}

impl<T> Deref for Box<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        let address = self as *const Box<T> as usize;
        let delta = self.delta.get();
        unsafe {
            (address.wrapping_add_signed(delta) as *const T)
                .as_ref()
                .unwrap()
        }
    }
}

impl<T> DerefMut for Box<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        let address = self as *mut Box<T> as usize;
        let delta = self.delta.get();
        unsafe {
            (address.wrapping_add_signed(delta) as *mut T)
                .as_mut()
                .unwrap()
        }
    }
}
