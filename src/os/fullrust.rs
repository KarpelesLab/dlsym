//! fullrust-specific functionality: loading the system's libraries from a
//! libc-free static binary through [`dldota`].
//!
//! A [fullrust](https://github.com/KarpelesLab/fullrust) binary has no dynamic
//! loader of its own. On first use, dlsym boots the system's `ld.so` with
//! [`dldota`] and loads every library through it. The rest of the API works as
//! on Linux, with these differences:
//!
//! - Function symbols resolve to trampolines that switch to the foreign libc's
//!   thread pointer around each call, so they can be called directly. Data
//!   symbols resolve to their real address.
//! - A host function passed to foreign code (a callback, e.g. a `qsort`
//!   comparator) must be wrapped with [`Context::callback`] first; see
//!   [`context`].
//! - Foreign calls are serialized: one host thread at a time runs foreign code.
//! - [`Library::this`](crate::Library::this) is the foreign global scope: the
//!   system libc and libraries opened with `RTLD_GLOBAL`. It does not contain
//!   the host program's own symbols.
//! - [`AddressInfo::of`](crate::AddressInfo::of) and
//!   [`Library::path`](crate::Library::path) return `None`.
//! - Only x86_64 is supported by [`dldota`] so far.

use std::ffi::{CStr, CString, OsStr, c_char, c_int, c_void};
use std::os::unix::ffi::OsStrExt;
use std::ptr::NonNull;
use std::sync::{Mutex, OnceLock};

pub use dldota::Context;
use dldota::OpenFlags;

use super::Lookup;
use crate::{Error, Result};

/// `RTLD_NOW | RTLD_LOCAL`.
pub(crate) const DEFAULT_FLAGS: c_int = OpenFlags::NOW.0 | OpenFlags::LOCAL.0;

type DlSym = unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void;
type DlError = unsafe extern "C" fn() -> *const c_char;
type ErrnoLocation = unsafe extern "C" fn() -> *mut c_int;

/// The booted foreign world, and the foreign functions dlsym calls directly
/// (each through a thread-pointer-switching trampoline).
struct Foreign {
    ctx: Context,
    /// Raw `dlsym`/`dlerror`, for data symbols: [`Context::symbol_raw`]
    /// returns a trampoline even for data.
    dlsym: DlSym,
    dlerror: DlError,
    /// Serializes `dlsym` + `dlerror` pairs: every host thread shares one
    /// foreign thread, hence one `dlerror` slot.
    data_lookup: Mutex<()>,
    errno_location: Option<ErrnoLocation>,
}

impl Foreign {
    fn boot() -> std::result::Result<Self, String> {
        let ctx = Context::new().map_err(|e| format!("cannot boot the system loader: {e}"))?;
        let global = ctx
            .open_raw(None, OpenFlags::LAZY)
            .map_err(|e| e.to_string())?;
        let function = |name: &CStr| ctx.symbol_raw(global, name).map_err(|e| e.to_string());
        let dlsym = function(c"dlsym")?;
        let dlerror = function(c"dlerror")?;
        // glibc and musl both name it `__errno_location`.
        let errno_location = function(c"__errno_location").ok();
        // SAFETY: trampolines to the foreign functions of these signatures.
        let (dlsym, dlerror, errno_location) = unsafe {
            (
                std::mem::transmute::<NonNull<c_void>, DlSym>(dlsym),
                std::mem::transmute::<NonNull<c_void>, DlError>(dlerror),
                errno_location.map(|f| std::mem::transmute::<NonNull<c_void>, ErrnoLocation>(f)),
            )
        };
        Ok(Foreign {
            ctx,
            dlsym,
            dlerror,
            data_lookup: Mutex::new(()),
            errno_location,
        })
    }

    fn get() -> std::result::Result<&'static Foreign, &'static str> {
        static FOREIGN: OnceLock<std::result::Result<Foreign, String>> = OnceLock::new();
        FOREIGN
            .get_or_init(Foreign::boot)
            .as_ref()
            .map_err(String::as_str)
    }

    /// The pending foreign `dlerror` message, if any.
    fn take_error(&self) -> Option<String> {
        // SAFETY: `dlerror` has no preconditions.
        let msg = unsafe { (self.dlerror)() };
        // SAFETY: a non-null `dlerror` result is a valid C string, alive until
        // the next dl* call (excluded by `data_lookup`).
        (!msg.is_null()).then(|| {
            unsafe { CStr::from_ptr(msg) }
                .to_string_lossy()
                .into_owned()
        })
    }
}

/// The [`dldota`] context all libraries are loaded through, booting the
/// system's dynamic loader on first use.
///
/// Use it to wrap host functions that foreign code calls back:
///
/// ```ignore
/// let ctx = dlsym::os::fullrust::context()?;
/// // SAFETY: `compare` is an `extern "C" fn`.
/// let compare = unsafe { ctx.callback(compare as extern "C" fn(_, _) -> _) }?;
/// // pass `*compare` to qsort
/// ```
pub fn context() -> Result<&'static Context> {
    Foreign::get()
        .map(|f| &f.ctx)
        .map_err(|message| Error::Open {
            path: Default::default(),
            message: message.to_owned(),
        })
}

/// An owned handle from the foreign `dlopen`.
#[derive(Debug)]
pub(crate) struct Handle(NonNull<c_void>);

// SAFETY: handles are process-global and the foreign dl* functions are
// thread-safe (dldota serializes them).
unsafe impl Send for Handle {}
// SAFETY: see above.
unsafe impl Sync for Handle {}

fn open_error(path: &OsStr, message: impl Into<String>) -> Error {
    Error::Open {
        path: path.to_owned(),
        message: message.into(),
    }
}

pub(crate) unsafe fn open(path: &OsStr, flags: c_int) -> Result<Handle> {
    let cpath = CString::new(path.as_bytes()).map_err(|e| Error::InteriorNul {
        position: e.nul_position(),
    })?;
    let foreign = Foreign::get().map_err(|m| open_error(path, m))?;
    foreign
        .ctx
        .open_raw(Some(&cpath), OpenFlags(flags))
        .map(Handle)
        .map_err(|e| open_error(path, e.to_string()))
}

pub(crate) fn this() -> Result<Handle> {
    let foreign = Foreign::get().map_err(|m| open_error(OsStr::new(""), m))?;
    // Another reference to the global scope, so closing it is balanced.
    foreign
        .ctx
        .open_raw(None, OpenFlags::LAZY)
        .map(Handle)
        .map_err(|e| open_error(OsStr::new(""), e.to_string()))
}

pub(crate) fn lookup(handle: &Handle, name: &CStr, function: bool) -> Lookup {
    let foreign = match Foreign::get() {
        Ok(foreign) => foreign,
        Err(message) => return Lookup::NotFound(message.to_owned()),
    };
    if function {
        // A trampoline that switches the thread pointer around the call.
        return match foreign.ctx.symbol_raw(handle.0, name) {
            Ok(thunk) => Lookup::Found(thunk.as_ptr()),
            Err(e) => Lookup::NotFound(e.to_string()),
        };
    }
    let _guard = foreign
        .data_lookup
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    // `dlsym` may legitimately return null; only `dlerror` tells.
    foreign.take_error();
    // SAFETY: the handle is live and `name` is a valid C string.
    let addr = unsafe { (foreign.dlsym)(handle.0.as_ptr(), name.as_ptr()) };
    if addr.is_null()
        && let Some(message) = foreign.take_error()
    {
        return Lookup::NotFound(message);
    }
    Lookup::Found(addr)
}

pub(crate) fn close(handle: Handle) -> Result<()> {
    let foreign = Foreign::get().map_err(|m| Error::Close {
        message: m.to_owned(),
    })?;
    foreign.ctx.close_raw(handle.0).map_err(|e| Error::Close {
        message: e.to_string(),
    })
}

/// The foreign libc's `errno` for the calling thread.
pub(crate) fn errno() -> c_int {
    match Foreign::get().ok().and_then(|f| f.errno_location) {
        Some(location) => {
            // SAFETY: no preconditions.
            let errno = unsafe { location() };
            // SAFETY: `__errno_location` returns the foreign thread's `errno`.
            unsafe { *errno }
        }
        None => 0,
    }
}

pub(crate) fn address_info(_addr: *const c_void) -> Option<crate::AddressInfo> {
    None
}

pub(crate) fn library_path(_handle: &Handle) -> Option<std::path::PathBuf> {
    None
}
