use std::borrow::Cow;
use std::ffi::{CStr, OsStr, OsString};
use std::fmt;
use std::mem::ManuallyDrop;
use std::sync::Arc;

use crate::os::{Lookup, imp};
use crate::{ArcSymbol, Error, Result, Symbol, SymbolName, SymbolType};

/// A loaded dynamic library.
///
/// The library is unloaded when this value is dropped (or explicitly with
/// [`close`](Library::close)). Symbols borrowed from it cannot outlive it.
///
/// # Libraries that cannot be unloaded
///
/// Borrowing stops *your* code from using a library after it is unloaded,
/// but some libraries leave things behind that keep running their code:
/// threads they started, thread-local destructors, `atexit` handlers, or
/// callbacks registered elsewhere. Unloading those crashes later, far from
/// the cause (libclang on Windows is one). Keep such libraries loaded for
/// the life of the process with [`leak`](Library::leak), a `static`, or
#[cfg_attr(
    unix,
    doc = "[`OpenOptions::no_delete`](crate::os::unix::OpenOptions::no_delete)."
)]
#[cfg_attr(not(unix), doc = "`os::unix::OpenOptions::no_delete`.")]
pub struct Library {
    handle: ManuallyDrop<imp::Handle>,
}

impl Library {
    /// Loads the library at `path`, or searches for it by name using the
    /// platform's rules when `path` contains no directory separator.
    ///
    /// On Unix this is `dlopen(path, RTLD_NOW | RTLD_LOCAL)`; use
    #[cfg_attr(
        unix,
        doc = "[`os::unix::OpenOptions`](crate::os::unix::OpenOptions) for other flags."
    )]
    #[cfg_attr(not(unix), doc = "`os::unix::OpenOptions` for other flags.")]
    /// On Windows this is `LoadLibraryExW(path, NULL, 0)`, with the system's
    /// error dialogs suppressed.
    ///
    /// # Safety
    ///
    /// Loading a library runs its initialisers (and, on Windows, `DllMain`),
    /// which may do anything. The caller must trust the library.
    pub unsafe fn open(path: impl AsRef<OsStr>) -> Result<Self> {
        // SAFETY: forwarded to the caller.
        unsafe { imp::open(path.as_ref(), imp::DEFAULT_FLAGS) }.map(Self::from_handle)
    }

    /// Tries each candidate in turn with [`open`](Library::open), returning
    /// the first library that loads, or the last error if none does.
    ///
    /// Useful for libraries whose file name differs across platforms or
    /// versions:
    ///
    /// ```no_run
    /// # use dlsym::Library;
    /// // SAFETY: zlib has no unsound initialisers.
    /// let zlib = unsafe { Library::open_first(["libz.so.1", "libz.1.dylib", "zlib1.dll"]) };
    /// ```
    ///
    /// # Safety
    ///
    /// Same as [`open`](Library::open), for every candidate.
    pub unsafe fn open_first<I>(candidates: I) -> Result<Self>
    where
        I: IntoIterator,
        I::Item: AsRef<OsStr>,
    {
        let mut last = None;
        for candidate in candidates {
            // SAFETY: forwarded to the caller.
            match unsafe { Self::open(candidate) } {
                Ok(lib) => return Ok(lib),
                Err(e) => last = Some(e),
            }
        }
        Err(last.unwrap_or_else(|| Error::Open {
            path: OsString::new(),
            message: "no candidate library names given".into(),
        }))
    }

    /// Returns a handle to the running program.
    ///
    /// On Unix this is `dlopen(NULL, ..)`, which looks symbols up in the main
    /// program and every library loaded with global visibility (including
    /// libc). On Windows it is the executable module, which only exposes
    /// symbols the executable itself exports.
    pub fn this() -> Result<Self> {
        imp::this().map(Self::from_handle)
    }

    pub(crate) fn from_handle(handle: imp::Handle) -> Self {
        Self {
            handle: ManuallyDrop::new(handle),
        }
    }

    pub(crate) fn handle(&self) -> &imp::Handle {
        &self.handle
    }

    /// Looks up a symbol, borrowing the library for as long as the returned
    /// [`Symbol`] is alive.
    ///
    /// # Safety
    ///
    /// `T` must match the symbol's actual type: the right signature and
    /// calling convention for functions, the right pointee for data. A
    /// mismatch is undefined behaviour once the symbol is used.
    pub unsafe fn get<T: SymbolType>(&self, name: impl SymbolName) -> Result<Symbol<'_, T>> {
        self.resolve(name).map(Symbol::new)
    }

    /// Looks up a symbol, keeping the library alive through the [`Arc`]
    /// rather than a borrow. Useful for storing symbols in `'static`
    /// contexts.
    ///
    /// # Safety
    ///
    /// Same as [`get`](Library::get).
    pub unsafe fn get_arc<T: SymbolType>(
        self: &Arc<Self>,
        name: impl SymbolName,
    ) -> Result<ArcSymbol<T>> {
        self.resolve(name)
            .map(|inner| ArcSymbol::new(inner, Arc::clone(self)))
    }

    fn resolve<T: SymbolType>(&self, name: impl SymbolName) -> Result<T> {
        let name = name.to_symbol_name()?;
        let lookup = imp::lookup(&self.handle, &name, T::IS_FN);
        convert(name, lookup)
    }

    /// Adopts a function pointer that this library (or one it loaded)
    /// handed out at run time instead of exporting by name: the result of
    /// `vkGetInstanceProcAddr` or `eglGetProcAddress`, a struct member, a
    /// vtable entry. The returned [`Symbol`] borrows the library like one
    /// from [`get`](Library::get).
    ///
    /// On most platforms this only attaches the lifetime. On fullrust, such
    /// a pointer jumps straight into foreign code and crashes when called
    /// from the host; this wraps it in the same trampoline `get` returns.
    /// Going through `wrap` keeps a loader for such an API free of `cfg`s.
    /// Data pointers, and pointers that already came from `get`, are
    /// returned as they are.
    ///
    /// ```no_run
    /// # use std::ffi::{c_char, c_void};
    /// # fn main() -> Result<(), dlsym::Error> {
    /// # use dlsym::Library;
    /// type GetProcAddr = unsafe extern "C" fn(*mut c_void, *const c_char) -> Option<VoidFn>;
    /// type VoidFn = unsafe extern "C" fn();
    /// type EnumerateVersion = unsafe extern "C" fn(*mut u32) -> i32;
    ///
    /// // SAFETY: the Vulkan loader has no unsound initialisers.
    /// let vulkan = unsafe { Library::open("libvulkan.so.1")? };
    /// // SAFETY: `vkGetInstanceProcAddr` has this signature.
    /// let get_proc = unsafe { vulkan.get::<GetProcAddr>(c"vkGetInstanceProcAddr")? };
    /// let name = c"vkEnumerateInstanceVersion";
    /// // SAFETY: a null instance is allowed for global commands.
    /// let raw = unsafe { get_proc.call((std::ptr::null_mut(), name.as_ptr())) }.unwrap();
    /// // SAFETY: Vulkan documents this signature for that name.
    /// let enumerate = unsafe { vulkan.wrap(std::mem::transmute::<VoidFn, EnumerateVersion>(raw))? };
    /// let mut version = 0;
    /// // SAFETY: `version` is a valid `uint32_t *`.
    /// unsafe { enumerate.call((&mut version,)) };
    /// # Ok(()) }
    /// ```
    ///
    /// # Safety
    ///
    /// `f` must point to code or data of type `T` that stays valid for as
    /// long as this library is loaded.
    pub unsafe fn wrap<T: SymbolType>(&self, f: T) -> Result<Symbol<'_, T>> {
        wrap(f).map(Symbol::new)
    }

    /// Like [`wrap`](Library::wrap), keeping the library alive through the
    /// [`Arc`] rather than a borrow.
    ///
    /// # Safety
    ///
    /// Same as [`wrap`](Library::wrap).
    pub unsafe fn wrap_arc<T: SymbolType>(self: &Arc<Self>, f: T) -> Result<ArcSymbol<T>> {
        wrap(f).map(|inner| ArcSymbol::new(inner, Arc::clone(self)))
    }

    /// Returns the path the library was loaded from, if the platform can
    /// tell (glibc, Apple platforms and Windows). For [`Library::this`], this
    /// is the executable.
    pub fn path(&self) -> Option<std::path::PathBuf> {
        crate::info::library_path(&self.handle)
    }

    /// Keeps the library loaded for the rest of the program, so symbols
    /// borrowed from it are `'static`.
    pub fn leak(self) -> &'static Library {
        Box::leak(Box::new(self))
    }

    /// Unloads the library, reporting any error from the loader. Dropping a
    /// `Library` does the same but ignores errors.
    pub fn close(self) -> Result<()> {
        let mut this = ManuallyDrop::new(self);
        // SAFETY: `this` is never dropped, so the handle is taken exactly once.
        let handle = unsafe { ManuallyDrop::take(&mut this.handle) };
        imp::close(handle)
    }
}

/// Makes a foreign function pointer callable from the host.
fn wrap<T: SymbolType>(f: T) -> Result<T> {
    let Some(addr) = std::ptr::NonNull::new(f.to_address()).filter(|_| T::IS_FN) else {
        return Ok(f);
    };
    let wrapped = imp::wrap(addr)?;
    // Wrapping never yields null, which is all a function type rejects.
    Ok(T::from_address(wrapped).unwrap_or(f))
}

/// Turns a raw lookup into a typed symbol or the matching error.
pub(crate) fn convert<T: SymbolType>(name: Cow<'_, CStr>, lookup: Lookup) -> Result<T> {
    match lookup {
        Lookup::Found(addr) => T::from_address(addr).ok_or_else(|| Error::NullSymbol {
            name: name.into_owned(),
        }),
        Lookup::NotFound(message) => Err(Error::SymbolNotFound {
            name: name.into_owned(),
            message,
        }),
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        // SAFETY: `drop` runs at most once and the field is not used after.
        let handle = unsafe { ManuallyDrop::take(&mut self.handle) };
        let _ = imp::close(handle);
    }
}

impl fmt::Debug for Library {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Library")
            .field("handle", &*self.handle)
            .finish()
    }
}

/// Returns the platform's conventional file name for a library called
/// `name`: `lib{name}.so` on most Unix systems, `lib{name}.dylib` on Apple
/// platforms, and `{name}.dll` on Windows.
///
/// On Linux, the unversioned `lib{name}.so` link usually only exists when
/// development packages are installed; prefer the versioned name (e.g.
/// `libz.so.1`) with [`Library::open_first`] for libraries you ship against.
pub fn library_filename(name: impl AsRef<OsStr>) -> OsString {
    let (prefix, suffix) = if cfg!(windows) {
        ("", ".dll")
    } else if cfg!(target_vendor = "apple") {
        ("lib", ".dylib")
    } else {
        ("lib", ".so")
    };
    let mut out = OsString::from(prefix);
    out.push(name);
    out.push(suffix);
    out
}
