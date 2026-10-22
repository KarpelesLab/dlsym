//! The `library!` macro.
#![allow(missing_docs, clippy::multiple_unsafe_ops_per_block)]

use std::ffi::{CStr, c_char, c_int, c_long};

use dlsym::{Bindings, Error, Library};

dlsym::library! {
    /// Bindings exercising every kind of item.
    pub struct LibC {
        unsafe extern "C" {
            /// Absolute value.
            pub safe fn labs(x: c_long) -> c_long;
            pub fn strlen(s: *const c_char) -> usize;
            pub unsafe fn strcmp(a: *const c_char, b: *const c_char,) -> c_int;
            pub fn sprintf(buf: *mut c_char, fmt: *const c_char, ...) -> c_int;
            #[link_name = "strlen"]
            pub fn string_length(s: *const c_char) -> usize;
            #[optional]
            pub fn dlsym_definitely_missing(x: c_int) -> c_int;
            #[optional]
            pub safe fn abs(x: c_int) -> c_int;
            #[optional]
            pub fn dlsym_missing_variadic(x: c_int, ...);
            #[optional]
            pub static dlsym_missing_static: c_int;
            fn r#free(p: *mut std::ffi::c_void);
            /// Never looked up: disabled by `cfg`.
            #[cfg(any())]
            pub fn dlsym_disabled_by_cfg();
            #[cfg(all())]
            #[link_name = "strlen"]
            pub fn strlen_enabled_by_cfg(s: *const c_char) -> usize;
        }
    }
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
dlsym::library! {
    struct Glibc {
        unsafe extern "C" {
            static program_invocation_name: *const c_char;
            #[link_name = "program_invocation_name"]
            static mut invocation_name_mut: *const c_char;
            #[link_name = "program_invocation_name"]
            safe static invocation_name_addr: usize;
        }
    }
}

dlsym::library! {
    struct Empty {}
}

fn libc() -> LibC {
    #[cfg(unix)]
    let lib = Library::this().unwrap();
    // SAFETY: msvcrt is a system library.
    #[cfg(windows)]
    let lib = unsafe { Library::open("msvcrt.dll") }.unwrap();
    LibC::load(lib).unwrap()
}

#[test]
fn functions() {
    let c = libc();
    assert_eq!(c.labs(-7), 7);
    // SAFETY: valid C strings.
    unsafe {
        assert_eq!(c.strlen(c"hello".as_ptr()), 5);
        assert_eq!(c.string_length(c"hi".as_ptr()), 2);
        assert_eq!(c.strcmp(c"a".as_ptr(), c"a".as_ptr()), 0);
    }
    // SAFETY: valid C string.
    assert_eq!(unsafe { c.strlen_enabled_by_cfg(c"abc".as_ptr()) }, 3);
    // `r#free` resolves the `free` symbol.
    // SAFETY: freeing null is a no-op.
    unsafe { c.r#free(std::ptr::null_mut()) };
}

#[test]
fn variadic() {
    let c = libc();
    let mut buf = [0 as c_char; 16];
    // SAFETY: format matches the arguments; `f` does not escape.
    let n = unsafe {
        c.sprintf()
            .with(|f| f(buf.as_mut_ptr(), c"%03d".as_ptr(), 7))
    };
    assert_eq!(n, 3);
    // SAFETY: NUL-terminated by `sprintf`.
    assert_eq!(unsafe { CStr::from_ptr(buf.as_ptr()) }, c"007");
}

#[test]
fn optional() {
    let c = libc();
    // SAFETY: never actually called.
    assert_eq!(unsafe { c.dlsym_definitely_missing(1) }, None);
    assert_eq!(c.abs(-3), Some(3));
    assert!(c.dlsym_missing_variadic().is_none());
    // SAFETY: missing, so never dereferenced.
    assert!(unsafe { c.dlsym_missing_static() }.is_none());
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[test]
fn statics() {
    let g = Glibc::load(Library::this().unwrap()).unwrap();
    // SAFETY: glibc sets it before `main` and never changes it.
    let name = unsafe { CStr::from_ptr(*g.program_invocation_name()) };
    assert!(!name.to_bytes().is_empty());
    let ptr: *mut *const c_char = g.invocation_name_mut();
    // SAFETY: same object as above.
    assert_eq!(unsafe { *ptr }, unsafe { *g.program_invocation_name() });
    assert_ne!(*g.invocation_name_addr(), 0);
}

#[test]
fn missing_required_symbol() {
    dlsym::library! {
        struct Missing {
            unsafe extern "C" {
                fn dlsym_definitely_missing();
            }
        }
    }
    let err = Missing::load(libc_library()).unwrap_err();
    match err {
        Error::SymbolNotFound { name, .. } => {
            assert_eq!(name.as_c_str(), c"dlsym_definitely_missing")
        }
        e => panic!("{e:?}"),
    }
}

#[test]
fn empty_and_debug() {
    let e = Empty::load(libc_library()).unwrap();
    assert!(format!("{e:?}").starts_with("Empty { library: Library"));
    let _ = e.library();
}

#[test]
fn send_sync() {
    fn check<T: Send + Sync>() {}
    check::<LibC>();
}

fn libc_library() -> Library {
    #[cfg(unix)]
    return Library::this().unwrap();
    // SAFETY: msvcrt is a system library.
    #[cfg(windows)]
    return unsafe { Library::open("msvcrt.dll") }.unwrap();
}
