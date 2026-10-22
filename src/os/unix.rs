//! Unix-specific functionality (`dlopen` and friends).

use std::ffi::{CStr, CString, OsStr, c_int, c_void};
use std::os::unix::ffi::OsStrExt;
use std::ptr::NonNull;

use super::Lookup;
use crate::{Error, Library, Result};

/// Flags accepted by [`LibraryExt::open_with_flags`].
///
/// See `dlopen(3)` for their meaning. [`Library::open`] uses
/// `RTLD_NOW | RTLD_LOCAL`.
pub mod flags {
    use std::ffi::c_int;

    /// Resolve undefined symbols lazily, on first use.
    pub const RTLD_LAZY: c_int = libc::RTLD_LAZY;
    /// Resolve all undefined symbols before `dlopen` returns.
    pub const RTLD_NOW: c_int = libc::RTLD_NOW;
    /// Make symbols available to subsequently loaded libraries.
    pub const RTLD_GLOBAL: c_int = libc::RTLD_GLOBAL;
    /// Do not make symbols available to subsequently loaded libraries.
    pub const RTLD_LOCAL: c_int = libc::RTLD_LOCAL;
}

/// Unix-only extensions to [`Library`].
pub trait LibraryExt: Sized {
    /// Opens a library with explicit `dlopen` flags.
    ///
    /// # Safety
    ///
    /// Same as [`Library::open`]. Additionally, `RTLD_LAZY` defers symbol
    /// resolution failures to call time, where they abort the process.
    unsafe fn open_with_flags(path: impl AsRef<OsStr>, flags: c_int) -> Result<Self>;

    /// Returns a handle to the main program and the libraries it was linked
    /// against (`dlopen(NULL, ..)`).
    fn this() -> Result<Self>;
}

impl LibraryExt for Library {
    unsafe fn open_with_flags(path: impl AsRef<OsStr>, flags: c_int) -> Result<Self> {
        // SAFETY: forwarded to the caller.
        unsafe { open(Some(path.as_ref()), flags) }.map(Library::from_handle)
    }

    fn this() -> Result<Self> {
        // SAFETY: the main program and its dependencies are already loaded,
        // so no initialiser runs.
        unsafe { open(None, flags::RTLD_NOW | flags::RTLD_LOCAL) }.map(Library::from_handle)
    }
}

/// An owned `dlopen` handle.
#[derive(Debug)]
pub(crate) struct Handle(NonNull<c_void>);

// SAFETY: `dlopen` handles are process-global and the dl* functions are
// thread-safe on every supported Unix.
unsafe impl Send for Handle {}
// SAFETY: see above.
unsafe impl Sync for Handle {}

/// Takes the pending `dlerror` message for the current thread, if any.
fn take_error() -> Option<String> {
    // SAFETY: `dlerror` has no preconditions.
    let msg = unsafe { libc::dlerror() };
    if msg.is_null() {
        return None;
    }
    // SAFETY: a non-null `dlerror` result is a valid C string that stays
    // alive until the next dl* call on this thread.
    Some(
        unsafe { CStr::from_ptr(msg) }
            .to_string_lossy()
            .into_owned(),
    )
}

pub(crate) unsafe fn open(path: Option<&OsStr>, flags: c_int) -> Result<Handle> {
    let cpath = path
        .map(|p| CString::new(p.as_bytes()))
        .transpose()
        .map_err(|e| Error::InteriorNul {
            position: e.nul_position(),
        })?;
    let ptr = cpath.as_deref().map_or(std::ptr::null(), CStr::as_ptr);
    // SAFETY: `ptr` is null or a valid C string; running the library's
    // initialisers is the caller's responsibility.
    let handle = unsafe { libc::dlopen(ptr, flags) };
    NonNull::new(handle).map(Handle).ok_or_else(|| Error::Open {
        path: path.unwrap_or_default().to_owned(),
        message: take_error().unwrap_or_else(|| "unknown error".into()),
    })
}

pub(crate) fn lookup(handle: &Handle, name: &CStr) -> Lookup {
    // `dlsym` may legitimately return null, so the only reliable failure
    // signal is `dlerror`. Clear any stale message first.
    take_error();
    // SAFETY: the handle is live and `name` is a valid C string.
    let addr = unsafe { libc::dlsym(handle.0.as_ptr(), name.as_ptr()) };
    if addr.is_null()
        && let Some(message) = take_error()
    {
        return Lookup::NotFound(message);
    }
    Lookup::Found(addr)
}

pub(crate) fn close(handle: Handle) -> Result<()> {
    // SAFETY: the handle is live and ownership is consumed here, so it is
    // closed exactly once.
    if unsafe { libc::dlclose(handle.0.as_ptr()) } == 0 {
        Ok(())
    } else {
        Err(Error::Close {
            message: take_error().unwrap_or_else(|| "unknown error".into()),
        })
    }
}
