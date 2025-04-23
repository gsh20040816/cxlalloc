use std::io;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("out-of-bounds memory access")]
    OutOfBounds,

    #[error("ioctl error: {}", _0)]
    Ioctl(#[source] io::Error),

    #[error("ftruncate error: {}", _0)]
    Ftruncate(#[source] io::Error),

    #[error("mmap error: {}", _0)]
    Mmap(#[source] io::Error),

    #[error("munmap error: {}", _0)]
    Munmap(#[source] io::Error),

    #[error("mbind error: {}", _0)]
    Mbind(#[source] io::Error),

    #[error("madvise error: {}", _0)]
    Madvise(#[source] io::Error),

    #[error("shm_open error: {}", _0)]
    ShmOpen(#[source] io::Error),

    #[error("shm_unlink error: {}", _0)]
    ShmUnlink(#[source] io::Error),

    #[error(transparent)]
    Shm(#[from] shm::Error),

    #[error(transparent)]
    Io(#[from] io::Error),
}
