//! Declaring a whole API with `library!`.
//!
//! Run with `cargo run --example bindings`.

#![allow(clippy::multiple_unsafe_ops_per_block)]

use std::ffi::{CStr, c_char, c_int, c_long};

use dlsym::{Bindings, Library};

dlsym::library! {
    /// A few functions from the C library.
    pub struct LibC {
        unsafe extern "C" {
            /// Safe to call with any argument, so no `unsafe` needed.
            pub safe fn labs(x: c_long) -> c_long;
            pub fn strlen(s: *const c_char) -> usize;
            pub fn atoi(s: *const c_char) -> c_int;
            pub fn sprintf(buf: *mut c_char, fmt: *const c_char, ...) -> c_int;
            /// Not every C library has it: `None` when missing.
            #[optional]
            pub fn strlcpy(dst: *mut c_char, src: *const c_char, size: usize) -> usize;
        }
    }
}

fn main() -> dlsym::Result<()> {
    #[cfg(unix)]
    let lib = Library::this()?;
    // SAFETY: msvcrt is a system library.
    #[cfg(windows)]
    let lib = unsafe { Library::open("msvcrt.dll") }?;
    // Resolves every symbol now; a missing non-optional one is an error.
    let libc = LibC::load(lib)?;

    println!("labs(-42) = {}", libc.labs(-42));
    // SAFETY: valid C strings.
    unsafe {
        println!("strlen(\"dlsym\") = {}", libc.strlen(c"dlsym".as_ptr()));
        println!("atoi(\"123\") = {}", libc.atoi(c"123".as_ptr()));
    }

    let mut buf = [0 as c_char; 32];
    // SAFETY: the buffer is large enough and the format matches.
    unsafe {
        libc.sprintf()
            .with(|f| f(buf.as_mut_ptr(), c"%05.1f".as_ptr(), 2.5f64))
    };
    // SAFETY: `sprintf` NUL-terminated the buffer.
    println!("sprintf = {:?}", unsafe { CStr::from_ptr(buf.as_ptr()) });

    // SAFETY: never called with a null pointer here; we only check presence.
    match unsafe { libc.strlcpy(buf.as_mut_ptr(), c"x".as_ptr(), buf.len()) } {
        Some(n) => println!("strlcpy is available ({n})"),
        None => println!("strlcpy is not available in this C library"),
    }
    Ok(())
}
