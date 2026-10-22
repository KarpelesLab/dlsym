use std::ffi::c_void;
use std::fmt;
use std::marker::PhantomData;
use std::sync::Arc;

use crate::{FnPtr, Library, SymbolType};

/// A symbol borrowed from a [`Library`].
///
/// The `'lib` lifetime ties the symbol to its library, so the library cannot
/// be dropped while the symbol is reachable:
///
/// ```compile_fail,E0505
/// # use dlsym::Library;
/// # unsafe {
/// let lib = Library::open("libm.so.6").unwrap();
/// let cos = lib.get::<unsafe extern "C" fn(f64) -> f64>(c"cos").unwrap();
/// drop(lib); // error[E0505]: cannot move out of `lib` because it is borrowed
/// cos.call((0.0,));
/// # }
/// ```
///
/// Unlike a `Deref<Target = T>` design, the raw pointer is only reachable
/// through the `unsafe` [`into_inner`](Symbol::into_inner), so it cannot be
/// copied out and outlive the library by accident.
pub struct Symbol<'lib, T> {
    inner: T,
    _lib: PhantomData<&'lib Library>,
}

impl<T: SymbolType> Symbol<'_, T> {
    pub(crate) fn new(inner: T) -> Self {
        Self {
            inner,
            _lib: PhantomData,
        }
    }

    /// Returns the symbol's address.
    pub fn as_ptr(&self) -> *mut c_void {
        self.inner.to_address()
    }

    /// Detaches the symbol from its library's lifetime.
    ///
    /// # Safety
    ///
    /// The returned value must not be used after the library is unloaded.
    pub unsafe fn into_inner(self) -> T {
        self.inner
    }
}

impl<T: FnPtr> Symbol<'_, T> {
    /// Calls the function. Arguments are passed as a tuple.
    ///
    /// # Safety
    ///
    /// The call must satisfy the foreign function's contract.
    pub unsafe fn call(&self, args: T::Args) -> T::Output {
        // SAFETY: the library outlives `self`; the rest is the caller's.
        unsafe { self.inner.call(args) }
    }
}

impl<T: SymbolType> Clone for Symbol<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: SymbolType> Copy for Symbol<'_, T> {}

impl<T: SymbolType> fmt::Debug for Symbol<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Symbol").field(&self.as_ptr()).finish()
    }
}

/// A symbol that keeps its [`Library`] alive through an [`Arc`].
///
/// Obtained from [`Library::get_arc`]. The library is unloaded only once every
/// `ArcSymbol` and every other `Arc<Library>` clone is dropped.
pub struct ArcSymbol<T> {
    inner: T,
    lib: Arc<Library>,
}

impl<T: SymbolType> ArcSymbol<T> {
    pub(crate) fn new(inner: T, lib: Arc<Library>) -> Self {
        Self { inner, lib }
    }

    /// Returns the library this symbol keeps alive.
    pub fn library(&self) -> &Arc<Library> {
        &self.lib
    }

    /// Returns the symbol's address.
    pub fn as_ptr(&self) -> *mut c_void {
        self.inner.to_address()
    }

    /// Detaches the symbol from its library.
    ///
    /// # Safety
    ///
    /// The returned value must not be used after the library is unloaded.
    pub unsafe fn into_inner(self) -> T {
        self.inner
    }
}

impl<T: FnPtr> ArcSymbol<T> {
    /// Calls the function. Arguments are passed as a tuple.
    ///
    /// # Safety
    ///
    /// The call must satisfy the foreign function's contract.
    pub unsafe fn call(&self, args: T::Args) -> T::Output {
        // SAFETY: `self.lib` keeps the library loaded; the rest is the
        // caller's.
        unsafe { self.inner.call(args) }
    }
}

impl<T: SymbolType> Clone for ArcSymbol<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner,
            lib: Arc::clone(&self.lib),
        }
    }
}

impl<T: SymbolType> fmt::Debug for ArcSymbol<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ArcSymbol").field(&self.as_ptr()).finish()
    }
}
