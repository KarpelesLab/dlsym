use std::ffi::{CString, OsString};
use std::fmt;

/// Errors returned when loading libraries or resolving symbols.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// The library could not be opened.
    Open {
        /// The path or name that was passed to the loader.
        path: OsString,
        /// The loader's description of the failure.
        message: String,
    },
    /// The symbol was not found in the library.
    SymbolNotFound {
        /// The requested symbol name.
        name: CString,
        /// The loader's description of the failure.
        message: String,
    },
    /// The symbol exists but its address is null, and the requested type
    /// cannot represent null (e.g. a function pointer).
    NullSymbol {
        /// The requested symbol name.
        name: CString,
    },
    /// The library could not be closed.
    Close {
        /// The loader's description of the failure.
        message: String,
    },
    /// A library path or symbol name contained an interior NUL byte.
    InteriorNul {
        /// Byte offset of the offending NUL.
        position: usize,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Open { path, message } => {
                write!(f, "failed to open library {}: {message}", path.display())
            }
            Error::SymbolNotFound { name, message } => {
                write!(f, "symbol {name:?} not found: {message}")
            }
            Error::NullSymbol { name } => {
                write!(f, "symbol {name:?} resolved to a null address")
            }
            Error::Close { message } => write!(f, "failed to close library: {message}"),
            Error::InteriorNul { position } => {
                write!(f, "unexpected NUL byte at offset {position}")
            }
        }
    }
}

impl std::error::Error for Error {}
