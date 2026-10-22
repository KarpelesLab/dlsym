use std::ffi::OsStr;

use crate::{Library, Result};

/// Structs that bundle a [`Library`] with symbols resolved from it.
///
/// Implemented by the [`library!`](crate::library) macro. Because the struct
/// owns the library, the function pointers it stores can never dangle.
pub trait Bindings: Sized {
    /// Resolves every symbol from an already loaded library.
    fn load(library: Library) -> Result<Self>;

    /// Returns the underlying library.
    fn library(&self) -> &Library;

    /// Loads the library at `path` and resolves every symbol.
    ///
    /// If the bound library exports its own `open` function, the generated
    /// method shadows this one; call `<T as Bindings>::open` instead.
    ///
    /// # Safety
    ///
    /// Same as [`Library::open`].
    unsafe fn open(path: impl AsRef<OsStr>) -> Result<Self> {
        // SAFETY: forwarded to the caller.
        Self::load(unsafe { Library::open(path) }?)
    }
}

/// Declares a struct holding a [`Library`] and a set of functions resolved
/// from it when it is loaded.
///
/// Every function becomes an `unsafe` method with the same signature. Loading
/// fails if any function is missing, so a successfully constructed value can
/// call every method without further lookups.
///
/// ```no_run
/// use dlsym::Bindings;
///
/// dlsym::library! {
///     /// The C math library.
///     pub struct LibM {
///         /// Cosine.
///         pub fn cos(x: f64) -> f64;
///         pub fn sqrt(x: f64) -> f64;
///     }
/// }
///
/// # fn main() -> dlsym::Result<()> {
/// // SAFETY: libm has no unsound initialisers.
/// let libm = unsafe { LibM::open("libm.so.6") }?;
/// // SAFETY: `sqrt` is defined for every `f64`.
/// assert_eq!(unsafe { libm.sqrt(4.0) }, 2.0);
/// # Ok(()) }
/// ```
///
/// This is the shape emitted by `dlsym-gen` from C headers.
#[macro_export]
macro_rules! library {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident {
            $(
                $(#[$fmeta:meta])*
                $fvis:vis fn $func:ident ( $($arg:ident : $argty:ty),* $(,)? ) $(-> $ret:ty)? ;
            )*
        }
    ) => {
        $(#[$meta])*
        $vis struct $name {
            __dlsym_library: $crate::Library,
            $( $func: unsafe extern "C" fn($($argty),*) $(-> $ret)?, )*
        }

        impl $crate::Bindings for $name {
            fn load(library: $crate::Library) -> $crate::Result<Self> {
                $(
                    // SAFETY: the pointer is stored next to the library that
                    // owns it and is only reachable through `&self`, so it
                    // cannot outlive the library. The signature is asserted
                    // by the macro invocation and only relied upon by the
                    // `unsafe` methods below.
                    let $func = unsafe {
                        library
                            .get::<unsafe extern "C" fn($($argty),*) $(-> $ret)?>(const {
                                match ::core::ffi::CStr::from_bytes_with_nul(
                                    ::core::concat!(::core::stringify!($func), "\0").as_bytes(),
                                ) {
                                    ::core::result::Result::Ok(name) => name,
                                    ::core::result::Result::Err(_) => {
                                        ::core::panic!("symbol name contains a NUL byte")
                                    }
                                }
                            })?
                            .into_inner()
                    };
                )*
                ::core::result::Result::Ok(Self {
                    __dlsym_library: library,
                    $( $func, )*
                })
            }

            fn library(&self) -> &$crate::Library {
                &self.__dlsym_library
            }
        }

        impl $name {
            $(
                $(#[$fmeta])*
                ///
                /// # Safety
                ///
                /// The call must satisfy the foreign function's contract.
                #[allow(clippy::too_many_arguments)]
                $fvis unsafe fn $func(&self, $($arg: $argty),*) $(-> $ret)? {
                    // SAFETY: `self` owns the library, so the pointer is
                    // live; the rest is the caller's.
                    unsafe { (self.$func)($($arg),*) }
                }
            )*
        }
    };
}
