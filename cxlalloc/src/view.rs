use core::cell::UnsafeCell;

use crate::thread;

pub trait Lens {
    type Perspective;
    type Scope<'a, T: 'a>;

    unsafe fn focus<'a, T: 'a>(scope: Self::Scope<'a, T>, id: thread::Id) -> &'a mut T;
}

pub struct Unfocus {}

impl Lens for Unfocus {
    type Perspective = ();
    type Scope<'a, T: 'a> = &'a thread::Array<UnsafeCell<T>>;

    unsafe fn focus<'a, T: 'a>(scope: Self::Scope<'a, T>, id: thread::Id) -> &'a mut T {
        scope[id].get().as_mut().unwrap()
    }
}

pub struct Focus {}

impl Lens for Focus {
    type Perspective = thread::Id;
    type Scope<'a, T: 'a> = &'a mut T;

    unsafe fn focus<'a, T: 'a>(scope: Self::Scope<'a, T>, _: thread::Id) -> &'a mut T {
        scope
    }
}
