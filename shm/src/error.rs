use core::fmt::Display;
use std::io;

#[derive(Debug)]
pub enum Error {
    Libc {
        name: &'static str,
        source: io::Error,
    },
}

impl Error {
    pub(crate) fn is_not_found(&self) -> bool {
        match self {
            Error::Libc { name: _, source } => matches!(source.kind(), io::ErrorKind::NotFound),
        }
    }

    pub(crate) fn is_already_exists(&self) -> bool {
        match self {
            Error::Libc { name: _, source } => {
                matches!(source.kind(), io::ErrorKind::AlreadyExists)
            }
        }
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Libc { name, source: _ } => write!(f, "{name} error"),
        }
    }
}

impl core::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Libc { name: _, source } => Some(source),
        }
    }
}
