//! Loading a library and calling functions through borrowed symbols.
//!
//! Run with `cargo run --example symbols`.

#![allow(clippy::multiple_unsafe_ops_per_block)]

use std::ffi::{CStr, c_char, c_int};

use dlsym::Library;

#[cfg(target_os = "linux")]
const LIBM: &str = "libm.so.6";
#[cfg(target_vendor = "apple")]
const LIBM: &str = "/usr/lib/libm.dylib";
#[cfg(windows)]
const LIBM: &str = "ucrtbase.dll";
#[cfg(target_os = "freebsd")]
const LIBM: &str = "libm.so.5";
#[cfg(not(any(
    target_os = "linux",
    target_os = "freebsd",
    target_vendor = "apple",
    windows
)))]
const LIBM: &str = "libm.so";

fn main() -> dlsym::Result<()> {
    // SAFETY: the C math library's initialisers are trusted.
    let libm = unsafe { Library::open(LIBM) }?;

    // SAFETY: `cos` has this signature.
    let cos = unsafe { libm.get::<unsafe extern "C" fn(f64) -> f64>(c"cos") }?;
    // SAFETY: `cos` is defined for every `f64`.
    println!("cos(0) = {}", unsafe { cos.call((0.0,)) });

    // Variadic functions are called through `with`; the raw function
    // pointer cannot escape the closure without `unsafe`.
    #[cfg(unix)]
    let libc = Library::this()?;
    // SAFETY: msvcrt is a system library.
    #[cfg(windows)]
    let libc = unsafe { Library::open("msvcrt.dll") }?;
    // SAFETY: `sprintf` has this signature.
    let sprintf = unsafe {
        libc.get::<unsafe extern "C" fn(*mut c_char, *const c_char, ...) -> c_int>(c"sprintf")
    }?;
    let mut buf = [0 as c_char; 64];
    // SAFETY: the buffer is large enough and the format matches.
    unsafe {
        sprintf.with(|f| {
            f(
                buf.as_mut_ptr(),
                c"%s has %d symbols".as_ptr(),
                c"this example".as_ptr(),
                2,
            )
        })
    };
    // SAFETY: `sprintf` NUL-terminated the buffer.
    let text = unsafe { CStr::from_ptr(buf.as_ptr()) };
    println!("{}", text.to_string_lossy());

    // `cos` borrows `libm`: uncommenting this line makes the call below a
    // compile error instead of a use-after-unload.
    // drop(libm);
    // SAFETY: as above.
    println!("cos(pi) = {}", unsafe { cos.call((std::f64::consts::PI,)) });
    Ok(())
}
