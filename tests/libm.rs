//! Exercises the loader against the platform's C math library.

use std::sync::Arc;

use dlsym::{Bindings, Error, Library};

#[cfg(target_os = "linux")]
const LIBM: &str = "libm.so.6";
#[cfg(target_vendor = "apple")]
const LIBM: &str = "/usr/lib/libm.dylib";
#[cfg(windows)]
const LIBM: &str = "ucrtbase.dll";
#[cfg(not(any(target_os = "linux", target_vendor = "apple", windows)))]
const LIBM: &str = "libm.so";

type UnaryF64 = unsafe extern "C" fn(f64) -> f64;

fn libm() -> Library {
    // SAFETY: libm has no unsound initialisers.
    unsafe { Library::open(LIBM) }.expect("failed to open libm")
}

#[test]
fn borrowed_symbol() {
    let lib = libm();
    // SAFETY: `cos` has this signature.
    let cos = unsafe { lib.get::<UnaryF64>(c"cos") }.unwrap();
    // SAFETY: `cos` is defined for every `f64`.
    assert_eq!(unsafe { cos.call((0.0,)) }, 1.0);
    assert!(!cos.as_ptr().is_null());
}

#[test]
fn str_names() {
    let lib = libm();
    // SAFETY: `sqrt` has this signature.
    let sqrt = unsafe { lib.get::<UnaryF64>("sqrt") }.unwrap();
    // SAFETY: `sqrt` is defined for every `f64`.
    assert_eq!(unsafe { sqrt.call((9.0,)) }, 3.0);
}

#[test]
fn arc_symbol_outlives_local_handle() {
    let lib = Arc::new(libm());
    // SAFETY: `sqrt` has this signature.
    let sqrt = unsafe { lib.get_arc::<UnaryF64>(c"sqrt") }.unwrap();
    drop(lib);
    assert_eq!(Arc::strong_count(sqrt.library()), 1);
    // SAFETY: `sqrt` is defined for every `f64`; `sqrt` keeps libm loaded.
    assert_eq!(unsafe { sqrt.call((16.0,)) }, 4.0);
}

#[test]
fn missing_symbol() {
    let lib = libm();
    // SAFETY: never called.
    let err = unsafe { lib.get::<UnaryF64>(c"dlsym_definitely_not_a_symbol") }.unwrap_err();
    assert!(matches!(err, Error::SymbolNotFound { .. }), "{err:?}");
}

#[test]
fn missing_library() {
    // SAFETY: the library does not exist, so nothing is run.
    let err = unsafe { Library::open("libdlsym-definitely-missing.so") }.unwrap_err();
    assert!(matches!(err, Error::Open { .. }), "{err:?}");
}

#[test]
fn explicit_close() {
    libm().close().unwrap();
}

dlsym::library! {
    struct LibM {
        unsafe extern "C" {
            fn cos(x: f64) -> f64;
            fn pow(x: f64, y: f64) -> f64;
        }
    }
}

dlsym::library! {
    struct Broken {
        unsafe extern "C" {
            fn dlsym_definitely_not_a_symbol();
        }
    }
}

#[test]
fn library_macro() {
    // SAFETY: libm has no unsound initialisers.
    let m = unsafe { LibM::open(LIBM) }.unwrap();
    // SAFETY: `cos` is defined for every `f64`.
    assert_eq!(unsafe { m.cos(0.0) }, 1.0);
    // SAFETY: `pow` is defined for these inputs.
    assert_eq!(unsafe { m.pow(2.0, 10.0) }, 1024.0);
}

#[test]
fn library_macro_reports_missing_symbols() {
    let err = Broken::load(libm()).err().unwrap();
    assert!(matches!(err, Error::SymbolNotFound { .. }), "{err:?}");
}

#[cfg(unix)]
#[test]
fn this_process() {
    let me = Library::this().unwrap();
    // SAFETY: `strlen` has this signature and is linked into every Rust
    // program through libc.
    let strlen =
        unsafe { me.get::<unsafe extern "C" fn(*const std::ffi::c_char) -> usize>(c"strlen") }
            .unwrap();
    // SAFETY: the argument is a valid C string.
    assert_eq!(unsafe { strlen.call((c"hello".as_ptr(),)) }, 5);
}
