//! Platform-specific functionality.
//!
//! The portable API lives at the crate root. These modules expose extension
//! traits for loader features that only exist on one platform family.

#[cfg(unix)]
pub mod unix;
#[cfg(unix)]
pub(crate) use self::unix as imp;

#[cfg(windows)]
pub mod windows;
#[cfg(windows)]
pub(crate) use self::windows as imp;

#[cfg(not(any(unix, windows)))]
compile_error!("dlsym supports only Unix-like and Windows targets");

/// Outcome of a raw symbol lookup.
pub(crate) enum Lookup {
    /// The symbol exists. Its address may legitimately be null.
    Found(*mut std::ffi::c_void),
    /// The symbol does not exist; carries the loader's message.
    NotFound(String),
}
