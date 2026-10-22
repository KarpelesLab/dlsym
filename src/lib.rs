//! Memory-safe bindings around the platform's dynamic library loading
//! primitives (`dlopen`/`dlsym` on Unix, `LoadLibraryExW`/`GetProcAddress` on
//! Windows).
//!
//! The central guarantee of this crate is that **a symbol can never outlive
//! the library it was loaded from** through safe code:
//!
//! - [`Symbol<'lib, T>`](Symbol) borrows its [`Library`], so the borrow checker
//!   rejects dropping the library while a symbol is alive.
//! - [`ArcSymbol<T>`](ArcSymbol) keeps the library alive through an
//!   [`Arc`](std::sync::Arc) for when a borrow is impractical (e.g. `'static`
//!   storage), without resorting to `transmute`.
//! - Neither type hands out the raw function pointer through safe code:
//!   functions are invoked through [`Symbol::call`] (or [`Symbol::with`] for
//!   variadic ones), so the pointer cannot be copied out and kept past an
//!   unload by accident.
//! - The [`library!`] macro generates a struct that owns the library together
//!   with every resolved symbol, using Rust 2024 `unsafe extern` block
//!   syntax. This is the shape the `dlsym-gen` tool emits from C headers
//!   (see [`codegen`], behind the `codegen` feature).
//!
//! ```no_run
//! # fn main() -> Result<(), dlsym::Error> {
//! use dlsym::Library;
//!
//! // SAFETY: libm has no unsound initialisers.
//! let libm = unsafe { Library::open("libm.so.6")? };
//! // SAFETY: `cos` has this signature in libm.
//! let cos = unsafe { libm.get::<unsafe extern "C" fn(f64) -> f64>(c"cos")? };
//! // SAFETY: `cos` is a pure function, valid for any `f64`.
//! assert_eq!(unsafe { cos.call((0.0,)) }, 1.0);
//!
//! drop(libm); // fine here; moving this above the call would not compile
//! # Ok(()) }
//! ```
//!
//! # Platform notes
//!
//! - **Unloading.** Borrowing only protects your own use of a library. Some
//!   libraries leave threads, thread-local destructors or callbacks behind
//!   that crash once they are unloaded; see
//!   [Libraries that cannot be unloaded](Library#libraries-that-cannot-be-unloaded).
//! - **Thread-local data.** Looking up a C `__thread` / `_Thread_local`
//!   variable returns its address *for the calling thread*. Such a data
//!   symbol must only be used on the thread that looked it up.
//! - **Error reporting.** Lookups rely on `dlerror` to tell a missing
//!   symbol from one whose address is null. Its state is thread-local on
//!   glibc, musl, Apple platforms and the BSDs, so concurrent lookups from
//!   several threads are fine there.
//! - **Lazy binding.** With `RTLD_LAZY` (see `os::unix::OpenOptions`), a
//!   library whose own dependencies are missing may load fine and abort the
//!   process later when a function is first called. [`Library::open`] uses
//!   `RTLD_NOW` to report such problems upfront.

#[doc(hidden)]
#[path = "private.rs"]
pub mod __private;
#[cfg(feature = "codegen")]
pub mod codegen;
mod error;
mod info;
mod library;
mod macros;
mod name;
pub mod os;
mod symbol;
mod types;

pub use error::Error;
pub use info::AddressInfo;
pub use library::{Library, library_filename};
pub use macros::Bindings;
pub use name::SymbolName;
pub use symbol::{ArcSymbol, Symbol};
pub use types::{FnPtr, SymbolType};

/// Convenience alias for results returned by this crate.
pub type Result<T, E = Error> = std::result::Result<T, E>;
