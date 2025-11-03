use crate::thread;

#[repr(C)]
#[derive(ribbit::Pack, Copy, Clone, Debug)]
#[ribbit(size = 32)]
pub(crate) struct Remote {
    #[ribbit(size = 16)]
    pub(crate) owner: Option<thread::Id>,

    #[ribbit(size = 16)]
    pub(crate) free: u16,
}
