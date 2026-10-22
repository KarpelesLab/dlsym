use std::ffi::OsStr;
use std::fmt;
use std::mem::ManuallyDrop;
use std::sync::Arc;

use crate::os::{Lookup, imp};
use crate::{ArcSymbol, Error, Result, Symbol, SymbolName, SymbolType};

/// A loaded dynamic library.
///
/// The library is unloaded when this value is dropped (or explicitly with
/// [`close`](Library::close)). Symbols borrowed from it cannot outlive it.
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
        doc = "[`os::unix::LibraryExt`](crate::os::unix::LibraryExt) for other flags."
    )]
    #[cfg_attr(not(unix), doc = "`os::unix::LibraryExt` for other flags.")]
    ///
    /// # Safety
    ///
    /// Loading a library runs its initialisers (and, on Windows, `DllMain`),
    /// which may do anything. The caller must trust the library.
    pub unsafe fn open(path: impl AsRef<OsStr>) -> Result<Self> {
        #[cfg(unix)]
        // SAFETY: forwarded to the caller.
        let handle = unsafe {
            imp::open(
                Some(path.as_ref()),
                imp::flags::RTLD_NOW | imp::flags::RTLD_LOCAL,
            )
        }?;
        #[cfg(windows)]
        // SAFETY: forwarded to the caller.
        let handle = unsafe { imp::open(path.as_ref()) }?;
        Ok(Self::from_handle(handle))
    }

    pub(crate) fn from_handle(handle: imp::Handle) -> Self {
        Self {
            handle: ManuallyDrop::new(handle),
        }
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
        match imp::lookup(&self.handle, &name) {
            Lookup::Found(addr) => T::from_address(addr).ok_or_else(|| Error::NullSymbol {
                name: name.into_owned(),
            }),
            Lookup::NotFound(message) => Err(Error::SymbolNotFound {
                name: name.into_owned(),
                message,
            }),
        }
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
