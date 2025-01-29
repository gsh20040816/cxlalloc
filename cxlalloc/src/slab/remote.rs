use crate::thread;

#[repr(C)]
#[ribbit::pack(size = 32, debug)]
pub(crate) struct Remote<B> {
    #[ribbit(size = 16)]
    pub(crate) owner: Option<thread::Id>,

    #[ribbit(size = 16)]
    pub(crate) free: u16,
}
