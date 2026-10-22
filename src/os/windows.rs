//! Windows-specific functionality (`LoadLibraryExW` and friends).

use std::ffi::{CStr, OsStr, c_void};
use std::os::windows::ffi::OsStrExt;
use std::ptr::NonNull;

use windows_sys::Win32::Foundation::{FreeLibrary, GetLastError, HMODULE};
use windows_sys::Win32::System::Diagnostics::Debug::{
    FORMAT_MESSAGE_FROM_SYSTEM, FORMAT_MESSAGE_IGNORE_INSERTS, FormatMessageW,
    SEM_FAILCRITICALERRORS, SetThreadErrorMode,
};
use windows_sys::Win32::System::LibraryLoader::{
    GetModuleHandleExW, GetProcAddress, LOAD_LIBRARY_SEARCH_APPLICATION_DIR,
    LOAD_LIBRARY_SEARCH_DEFAULT_DIRS, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR,
    LOAD_LIBRARY_SEARCH_SYSTEM32, LOAD_LIBRARY_SEARCH_USER_DIRS, LOAD_WITH_ALTERED_SEARCH_PATH,
    LoadLibraryExW,
};

use super::Lookup;
use crate::{Error, Library, Result};

pub(crate) const DEFAULT_FLAGS: u32 = 0;

/// Options for opening a library with `LoadLibraryExW`.
///
/// The defaults match [`Library::open`]: no flags, i.e. the standard DLL
/// search order.
///
/// ```no_run
/// use dlsym::os::windows::OpenOptions;
///
/// // Only search the application directory and System32, never the
/// // current directory or PATH.
/// // SAFETY: the library has no unsound initialisers.
/// let lib = unsafe {
///     OpenOptions::new()
///         .search_application_dir(true)
///         .search_system32(true)
///         .open("example.dll")
/// }?;
/// # Ok::<(), dlsym::Error>(())
/// ```
#[derive(Debug, Clone, Default)]
pub struct OpenOptions {
    flags: u32,
}

impl OpenOptions {
    /// Returns the default options (no flags).
    pub fn new() -> Self {
        Self {
            flags: DEFAULT_FLAGS,
        }
    }

    fn set(&mut self, flag: u32, on: bool) -> &mut Self {
        if on {
            self.flags |= flag;
        } else {
            self.flags &= !flag;
        }
        self
    }

    /// Searches the directory containing the DLL being loaded for its
    /// dependencies (`LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR`). `path` must be
    /// absolute.
    pub fn search_dll_load_dir(&mut self, on: bool) -> &mut Self {
        self.set(LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR, on)
    }

    /// Searches the application's directory
    /// (`LOAD_LIBRARY_SEARCH_APPLICATION_DIR`).
    pub fn search_application_dir(&mut self, on: bool) -> &mut Self {
        self.set(LOAD_LIBRARY_SEARCH_APPLICATION_DIR, on)
    }

    /// Searches directories added with `AddDllDirectory`
    /// (`LOAD_LIBRARY_SEARCH_USER_DIRS`).
    pub fn search_user_dirs(&mut self, on: bool) -> &mut Self {
        self.set(LOAD_LIBRARY_SEARCH_USER_DIRS, on)
    }

    /// Searches `%windows%\system32` (`LOAD_LIBRARY_SEARCH_SYSTEM32`).
    pub fn search_system32(&mut self, on: bool) -> &mut Self {
        self.set(LOAD_LIBRARY_SEARCH_SYSTEM32, on)
    }

    /// Searches the application directory, user directories and System32
    /// (`LOAD_LIBRARY_SEARCH_DEFAULT_DIRS`).
    pub fn search_default_dirs(&mut self, on: bool) -> &mut Self {
        self.set(LOAD_LIBRARY_SEARCH_DEFAULT_DIRS, on)
    }

    /// Uses the altered search order, starting from the DLL's directory
    /// (`LOAD_WITH_ALTERED_SEARCH_PATH`). `path` must be absolute.
    pub fn altered_search_path(&mut self, on: bool) -> &mut Self {
        self.set(LOAD_WITH_ALTERED_SEARCH_PATH, on)
    }

    /// Replaces all flags with a raw `LoadLibraryExW` flag set.
    pub fn custom_flags(&mut self, flags: u32) -> &mut Self {
        self.flags = flags;
        self
    }

    /// Opens the library at `path` with these options.
    ///
    /// # Safety
    ///
    /// Same as [`Library::open`].
    pub unsafe fn open(&self, path: impl AsRef<OsStr>) -> Result<Library> {
        // SAFETY: forwarded to the caller.
        unsafe { open(path.as_ref(), self.flags) }.map(Library::from_handle)
    }
}

/// Windows-only extensions to [`Library`].
pub trait LibraryExt: Sized {
    /// Returns a new reference to a module that is already loaded in the
    /// process, without loading it if it isn't (`GetModuleHandleExW`).
    fn open_already_loaded(name: impl AsRef<OsStr>) -> Result<Self>;
}

impl LibraryExt for Library {
    fn open_already_loaded(name: impl AsRef<OsStr>) -> Result<Self> {
        let wide = to_wide(name.as_ref())?;
        module_handle(wide.as_ptr()).map_or_else(
            || {
                Err(Error::Open {
                    path: name.as_ref().to_owned(),
                    message: last_error(),
                })
            },
            |h| Ok(Library::from_handle(h)),
        )
    }
}

/// An owned `HMODULE`.
#[derive(Debug)]
pub(crate) struct Handle(NonNull<c_void>);

// SAFETY: module handles are process-global and the loader is thread-safe.
unsafe impl Send for Handle {}
// SAFETY: see above.
unsafe impl Sync for Handle {}

/// Formats the calling thread's last error code.
fn last_error() -> String {
    // SAFETY: `GetLastError` has no preconditions.
    let code = unsafe { GetLastError() };
    let mut buf = [0u16; 512];
    // SAFETY: `buf` is writable for `buf.len()` UTF-16 units.
    let len = unsafe {
        FormatMessageW(
            FORMAT_MESSAGE_FROM_SYSTEM | FORMAT_MESSAGE_IGNORE_INSERTS,
            std::ptr::null(),
            code,
            0,
            buf.as_mut_ptr(),
            buf.len() as u32,
            std::ptr::null(),
        )
    };
    if len == 0 {
        return format!("error code {code}");
    }
    String::from_utf16_lossy(&buf[..len as usize])
        .trim_end()
        .to_owned()
}

fn to_wide(s: &OsStr) -> Result<Vec<u16>> {
    let wide: Vec<u16> = s.encode_wide().chain(Some(0)).collect();
    match wide[..wide.len() - 1].iter().position(|&c| c == 0) {
        Some(position) => Err(Error::InteriorNul { position }),
        None => Ok(wide),
    }
}

/// `GetModuleHandleExW(0, name, ..)`: takes a new reference to a module that
/// is already loaded. A null `name` means the executable.
fn module_handle(name: *const u16) -> Option<Handle> {
    let mut module: HMODULE = std::ptr::null_mut();
    // SAFETY: `name` is null or NUL-terminated and `module` is writable.
    // Flag 0 increments the module's reference count, which `close` releases.
    let ok = unsafe { GetModuleHandleExW(0, name, &mut module) };
    if ok == 0 {
        return None;
    }
    NonNull::new(module).map(Handle)
}

pub(crate) unsafe fn open(path: &OsStr, flags: u32) -> Result<Handle> {
    let wide = to_wide(path)?;
    // Suppress the "missing DLL" error dialog for this thread while loading.
    let mut previous = 0;
    // SAFETY: `previous` is writable.
    let restore = unsafe { SetThreadErrorMode(SEM_FAILCRITICALERRORS, &mut previous) } != 0;
    // SAFETY: `wide` is NUL-terminated; running `DllMain` is the caller's
    // responsibility.
    let handle = unsafe { LoadLibraryExW(wide.as_ptr(), std::ptr::null_mut(), flags) };
    // Capture the error before `SetThreadErrorMode` can overwrite it.
    let result = NonNull::new(handle).map(Handle).ok_or_else(|| Error::Open {
        path: path.to_owned(),
        message: last_error(),
    });
    if restore {
        // SAFETY: restores the mode saved above.
        unsafe { SetThreadErrorMode(previous, std::ptr::null_mut()) };
    }
    result
}

pub(crate) fn this() -> Result<Handle> {
    module_handle(std::ptr::null()).ok_or_else(|| Error::Open {
        path: Default::default(),
        message: last_error(),
    })
}

pub(crate) fn lookup(handle: &Handle, name: &CStr) -> Lookup {
    // SAFETY: the handle is live and `name` is a valid C string.
    match unsafe { GetProcAddress(handle.0.as_ptr(), name.as_ptr().cast()) } {
        Some(f) => Lookup::Found(f as *mut c_void),
        None => Lookup::NotFound(last_error()),
    }
}

pub(crate) fn close(handle: Handle) -> Result<()> {
    // SAFETY: the handle is live and ownership is consumed here, so it is
    // freed exactly once.
    if unsafe { FreeLibrary(handle.0.as_ptr()) } != 0 {
        Ok(())
    } else {
        Err(Error::Close {
            message: last_error(),
        })
    }
}
