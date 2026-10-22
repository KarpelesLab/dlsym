//! C front end: headers → [`ir`](super::ir).
//!
//! Planned implementation: libclang, itself loaded at runtime through this
//! crate's [`Library`](crate::Library), so no LLVM is needed at build time, and
//! whatever libclang the system provides is used. See `ROADMAP.md`.

use std::fmt;
use std::path::Path;

use super::ir::Header;

/// Errors produced while parsing headers.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// The front end is not implemented yet.
    Unimplemented,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Unimplemented => f.write_str("the C header parser is not implemented yet"),
        }
    }
}

impl std::error::Error for Error {}

/// Parses `path` with the given extra compiler arguments (`-I`, `-D`, ...).
pub fn parse_header(path: &Path, clang_args: &[String]) -> Result<Header, Error> {
    let _ = (path, clang_args);
    Err(Error::Unimplemented)
}
