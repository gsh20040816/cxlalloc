use std::io;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("out-of-bounds memory access")]
    OutOfBounds,

    #[error("mmap error: {}", _0)]
    Mmap(#[source] io::Error),

    #[error("munmap error: {}", _0)]
    Munmap(#[source] io::Error),

    #[error("mbind error: {}", _0)]
    Mbind(#[source] io::Error),

    #[error("madvise error: {}", _0)]
    Madvise(#[source] io::Error),

    #[error(transparent)]
    Io(#[from] io::Error),
}
