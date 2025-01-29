use crate::thread;

#[repr(C)]
#[ribbit::pack(size = 40, debug)]
pub(crate) struct Remote<B> {
    #[ribbit(size = 8)]
    pub(crate) class: B,

    #[ribbit(size = 16)]
    pub(crate) owner: Option<thread::Id>,

    pub(crate) free: u16,
}
