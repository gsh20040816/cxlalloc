#[cfg(feature = "validate")]
pub use checked::UnsafeCell;

#[cfg(not(feature = "validate"))]
pub use unchecked::UnsafeCell;

mod unchecked {
    #[repr(transparent)]
    pub struct UnsafeCell<T>(core::cell::UnsafeCell<T>);

    impl<T> UnsafeCell<T> {
        pub const fn new(value: T) -> Self {
            Self(core::cell::UnsafeCell::new(value))
        }

        #[inline]
        pub unsafe fn with<F: FnOnce(&T) -> U, U>(&self, apply: F) -> U {
            apply(&*self.0.get())
        }

        #[inline]
        pub unsafe fn with_mut<F: FnOnce(&mut T) -> U, U>(&self, apply: F) -> U {
            apply(&mut *self.0.get())
        }
    }
}

mod checked {
    use atomic_refcell::AtomicRefCell;

    #[repr(transparent)]
    pub struct UnsafeCell<T>(AtomicRefCell<T>);

    impl<T> UnsafeCell<T> {
        pub const fn new(value: T) -> Self {
            Self(AtomicRefCell::new(value))
        }

        #[inline]
        pub unsafe fn with<F: FnOnce(&T) -> U, U>(&self, apply: F) -> U {
            apply(&*self.0.borrow())
        }

        #[inline]
        pub unsafe fn with_mut<F: FnOnce(&mut T) -> U, U>(&self, apply: F) -> U {
            apply(&mut *self.0.borrow_mut())
        }
    }
}
