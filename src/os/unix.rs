//! Unix-specific functionality (`dlopen` and friends).

use std::ffi::{CStr, CString, OsStr, c_int, c_void};
use std::os::unix::ffi::OsStrExt;
use std::ptr::NonNull;

use super::Lookup;
use crate::{Error, Library, Result};

pub(crate) const DEFAULT_FLAGS: c_int = libc::RTLD_NOW | libc::RTLD_LOCAL;

/// Options for opening a library with `dlopen`.
///
/// The defaults match [`Library::open`]: `RTLD_NOW | RTLD_LOCAL`.
///
/// ```no_run
/// use dlsym::os::unix::OpenOptions;
///
/// // SAFETY: libm has no unsound initialisers.
/// let libm = unsafe { OpenOptions::new().global(true).open("libm.so.6") }?;
/// # Ok::<(), dlsym::Error>(())
/// ```
#[derive(Debug, Clone)]
pub struct OpenOptions {
    flags: c_int,
}

impl Default for OpenOptions {
    fn default() -> Self {
        Self::new()
    }
}

impl OpenOptions {
    /// Returns the default options (`RTLD_NOW | RTLD_LOCAL`).
    pub fn new() -> Self {
        Self {
            flags: DEFAULT_FLAGS,
        }
    }

    fn set(&mut self, flag: c_int, on: bool) -> &mut Self {
        if on {
            self.flags |= flag;
        } else {
            self.flags &= !flag;
        }
        self
    }

    /// Resolves undefined symbols on first use (`RTLD_LAZY`) instead of
    /// before `open` returns (`RTLD_NOW`).
    ///
    /// A symbol that then fails to resolve aborts the process when called,
    /// rather than making `open` fail.
    pub fn lazy(&mut self, lazy: bool) -> &mut Self {
        self.set(libc::RTLD_NOW, !lazy).set(libc::RTLD_LAZY, lazy)
    }

    /// Makes the library's symbols available to libraries loaded later
    /// (`RTLD_GLOBAL`) instead of keeping them private (`RTLD_LOCAL`).
    pub fn global(&mut self, global: bool) -> &mut Self {
        self.set(libc::RTLD_LOCAL, !global)
            .set(libc::RTLD_GLOBAL, global)
    }

    /// Never unloads the library, even once every handle is closed
    /// (`RTLD_NODELETE`). Use this for libraries that are unsound to unload,
    /// e.g. ones that register thread-local destructors.
    #[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
    pub fn no_delete(&mut self, no_delete: bool) -> &mut Self {
        self.set(libc::RTLD_NODELETE, no_delete)
    }

    /// Only succeeds if the library is already loaded (`RTLD_NOLOAD`), in
    /// which case its reference count is incremented.
    #[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
    pub fn no_load(&mut self, no_load: bool) -> &mut Self {
        self.set(libc::RTLD_NOLOAD, no_load)
    }

    /// Prefers the library's own symbols over global ones with the same name
    /// (`RTLD_DEEPBIND`, glibc only).
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    pub fn deep_bind(&mut self, deep_bind: bool) -> &mut Self {
        self.set(libc::RTLD_DEEPBIND, deep_bind)
    }

    /// Replaces all flags with a raw `dlopen` flag set.
    pub fn custom_flags(&mut self, flags: c_int) -> &mut Self {
        self.flags = flags;
        self
    }

    /// Opens the library at `path` with these options.
    ///
    /// # Safety
    ///
    /// Same as [`Library::open`]. With [`lazy`](OpenOptions::lazy), calling a
    /// function whose dependencies fail to resolve aborts the process.
    pub unsafe fn open(&self, path: impl AsRef<OsStr>) -> Result<Library> {
        // SAFETY: forwarded to the caller.
        unsafe { open(path.as_ref(), self.flags) }.map(Library::from_handle)
    }
}

/// Unix-only extensions to [`Library`].
pub trait LibraryExt {
    /// Looks up a specific version of a symbol (`dlvsym`), such as
    /// `("memcpy", "GLIBC_2.2.5")`.
    ///
    /// # Safety
    ///
    /// Same as [`Library::get`].
    #[cfg(any(all(target_os = "linux", target_env = "gnu"), target_os = "freebsd"))]
    unsafe fn get_versioned<T: crate::SymbolType>(
        &self,
        name: impl crate::SymbolName,
        version: impl crate::SymbolName,
    ) -> Result<crate::Symbol<'_, T>>;
}

impl LibraryExt for Library {
    #[cfg(any(all(target_os = "linux", target_env = "gnu"), target_os = "freebsd"))]
    unsafe fn get_versioned<T: crate::SymbolType>(
        &self,
        name: impl crate::SymbolName,
        version: impl crate::SymbolName,
    ) -> Result<crate::Symbol<'_, T>> {
        let name = name.to_symbol_name()?;
        let version = version.to_symbol_name()?;
        take_error();
        // SAFETY: the handle is live and both names are valid C strings.
        let addr =
            unsafe { libc::dlvsym(self.handle().0.as_ptr(), name.as_ptr(), version.as_ptr()) };
        let lookup = match (addr.is_null(), take_error()) {
            (true, Some(message)) => Lookup::NotFound(message),
            _ => Lookup::Found(addr),
        };
        crate::library::convert(name, lookup).map(crate::Symbol::new)
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

unsafe fn raw_open(path: Option<&CStr>, flags: c_int) -> Option<Handle> {
    let ptr = path.map_or(std::ptr::null(), CStr::as_ptr);
    // SAFETY: `ptr` is null or a valid C string; running the library's
    // initialisers is the caller's responsibility.
    NonNull::new(unsafe { libc::dlopen(ptr, flags) }).map(Handle)
}

pub(crate) unsafe fn open(path: &OsStr, flags: c_int) -> Result<Handle> {
    let cpath = CString::new(path.as_bytes()).map_err(|e| Error::InteriorNul {
        position: e.nul_position(),
    })?;
    // SAFETY: forwarded to the caller.
    unsafe { raw_open(Some(&cpath), flags) }.ok_or_else(|| Error::Open {
        path: path.to_owned(),
        message: take_error().unwrap_or_else(|| "unknown error".into()),
    })
}

pub(crate) fn this() -> Result<Handle> {
    // SAFETY: the main program and its dependencies are already loaded,
    // so no initialiser runs.
    unsafe { raw_open(None, DEFAULT_FLAGS) }.ok_or_else(|| Error::Open {
        path: Default::default(),
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

pub(crate) fn address_info(addr: *const c_void) -> Option<crate::AddressInfo> {
    // SAFETY: `Dl_info` is plain data; all-zero is a valid initial value.
    let mut info: libc::Dl_info = unsafe { std::mem::zeroed() };
    // SAFETY: `dladdr` only reads `addr` as a number and writes `info`.
    if unsafe { libc::dladdr(addr, &mut info) } == 0 || info.dli_fname.is_null() {
        return None;
    }
    // SAFETY: on success, `dli_fname` (checked above) is a valid C string
    // owned by the loader.
    let library = unsafe { CStr::from_ptr(info.dli_fname) };
    let library = std::ffi::OsStr::from_bytes(library.to_bytes()).into();
    // SAFETY: as above, for a non-null `dli_sname`.
    let symbol =
        (!info.dli_sname.is_null()).then(|| unsafe { CStr::from_ptr(info.dli_sname) }.to_owned());
    Some(crate::AddressInfo {
        library,
        library_base: info.dli_fbase,
        symbol,
        symbol_address: info.dli_saddr,
    })
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
pub(crate) fn library_path(handle: &Handle) -> Option<std::path::PathBuf> {
    // The public prefix of glibc's `struct link_map`.
    #[repr(C)]
    struct LinkMap {
        l_addr: usize,
        l_name: *const std::ffi::c_char,
    }
    let mut map: *const LinkMap = std::ptr::null();
    // SAFETY: the handle is live and `RTLD_DI_LINKMAP` writes a
    // `struct link_map *` into `map`.
    let rc = unsafe {
        libc::dlinfo(
            handle.0.as_ptr(),
            libc::RTLD_DI_LINKMAP,
            (&raw mut map).cast(),
        )
    };
    if rc != 0 || map.is_null() {
        take_error();
        return None;
    }
    // SAFETY: glibc keeps the link map alive while the library is loaded.
    let name = unsafe { (*map).l_name };
    // SAFETY: `l_name` is a valid C string.
    let name = unsafe { CStr::from_ptr(name) };
    if name.is_empty() {
        // The main program has an empty name.
        return std::env::current_exe().ok();
    }
    Some(std::ffi::OsStr::from_bytes(name.to_bytes()).into())
}

#[cfg(target_vendor = "apple")]
// One invariant covers every call in the block below.
#[allow(clippy::multiple_unsafe_ops_per_block)]
pub(crate) fn library_path(handle: &Handle) -> Option<std::path::PathBuf> {
    // There is no `dlinfo`: find the loaded image whose handle matches.
    // SAFETY: the dyld image list functions have no preconditions; each
    // `dlopen(.., RTLD_NOLOAD)` that succeeds is balanced by a `dlclose`.
    unsafe {
        for i in 0..libc::_dyld_image_count() {
            let name = libc::_dyld_get_image_name(i);
            if name.is_null() {
                continue;
            }
            let h = libc::dlopen(name, libc::RTLD_NOLOAD | libc::RTLD_LAZY);
            if h.is_null() {
                continue;
            }
            libc::dlclose(h);
            if h == handle.0.as_ptr() {
                return Some(std::ffi::OsStr::from_bytes(CStr::from_ptr(name).to_bytes()).into());
            }
        }
        // `dlopen(NULL)` returns a special handle for the main program.
        let main = libc::dlopen(std::ptr::null(), libc::RTLD_NOLOAD | libc::RTLD_LAZY);
        if !main.is_null() {
            libc::dlclose(main);
            if main == handle.0.as_ptr() {
                return std::env::current_exe().ok();
            }
        }
    }
    None
}

#[cfg(not(any(all(target_os = "linux", target_env = "gnu"), target_vendor = "apple")))]
pub(crate) fn library_path(_handle: &Handle) -> Option<std::path::PathBuf> {
    None
}
