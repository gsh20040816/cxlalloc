#[repr(C)]
#[derive(ribbit::Pack, Copy, Clone, Debug)]
#[ribbit(size = 16)]
pub(crate) struct Remote {
    #[ribbit(size = 16)]
    pub(crate) free: u16,
}
