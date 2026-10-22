//! Loader features beyond basic lookups.
#![allow(clippy::multiple_unsafe_ops_per_block)]

use std::ffi::{CStr, c_char, c_int};

use dlsym::{Error, Library, library_filename};

#[cfg(unix)]
fn libc() -> Library {
    Library::this().unwrap()
}

#[cfg(windows)]
fn libc() -> Library {
    // SAFETY: msvcrt is a system library.
    unsafe { Library::open("msvcrt.dll") }.unwrap()
}

#[test]
fn variadic_call() {
    let lib = libc();
    // SAFETY: `sprintf` has this signature.
    let sprintf = unsafe {
        lib.get::<unsafe extern "C" fn(*mut c_char, *const c_char, ...) -> c_int>(c"sprintf")
    }
    .unwrap();
    let mut buf = [0 as c_char; 32];
    // SAFETY: the buffer is large enough and the format matches the
    // arguments; `f` does not escape the closure.
    let n = unsafe { sprintf.with(|f| f(buf.as_mut_ptr(), c"%d-%s".as_ptr(), 42, c"ok".as_ptr())) };
    assert_eq!(n, 5);
    // SAFETY: `sprintf` NUL-terminated the buffer.
    assert_eq!(unsafe { CStr::from_ptr(buf.as_ptr()) }, c"42-ok");
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[test]
fn data_symbol() {
    let lib = libc();
    // SAFETY: glibc exports `program_invocation_name` as a `char *`.
    let name = unsafe { lib.get::<*mut *const c_char>(c"program_invocation_name") }.unwrap();
    // SAFETY: glibc initialises it before `main` and never frees it.
    let name = unsafe { CStr::from_ptr(*name.as_ref()) };
    assert!(!name.to_bytes().is_empty());
}

#[cfg(all(target_os = "linux", target_env = "gnu", target_arch = "x86_64"))]
#[test]
fn versioned_symbol() {
    use dlsym::os::unix::LibraryExt;

    let lib = libc();
    // SAFETY: never called.
    let memcpy = unsafe { lib.get_versioned::<*const u8>(c"memcpy", c"GLIBC_2.14") }.unwrap();
    assert!(!memcpy.as_ptr().is_null());
    // SAFETY: never called.
    let err = unsafe { lib.get_versioned::<*const u8>(c"memcpy", c"DLSYM_9.9") }.unwrap_err();
    assert!(matches!(err, Error::SymbolNotFound { .. }), "{err:?}");
}

#[cfg(target_os = "linux")]
#[test]
fn open_options() {
    use dlsym::os::unix::OpenOptions;

    // SAFETY: libm has no unsound initialisers.
    let libm = unsafe { OpenOptions::new().lazy(true).global(true).open("libm.so.6") }.unwrap();
    // `no_load` only succeeds because `libm` above keeps it loaded.
    // SAFETY: as above.
    let again = unsafe { OpenOptions::new().no_load(true).open("libm.so.6") }.unwrap();
    drop(again);
    drop(libm);
    // SAFETY: nothing is loaded.
    let missing = unsafe { OpenOptions::new().no_load(true).open("libdlsym-missing.so") };
    assert!(missing.is_err());
}

#[cfg(windows)]
#[test]
fn already_loaded() {
    use dlsym::os::windows::LibraryExt;

    let kernel32 = Library::open_already_loaded("kernel32.dll").unwrap();
    // SAFETY: never called.
    let f = unsafe { kernel32.get::<*const u8>(c"GetLastError") }.unwrap();
    assert!(!f.as_ptr().is_null());
    assert!(Library::open_already_loaded("dlsym-definitely-missing.dll").is_err());
}

#[test]
fn open_first_returns_first_loadable() {
    let candidates = [
        "libdlsym-missing-1",
        #[cfg(target_os = "linux")]
        "libm.so.6",
        #[cfg(target_vendor = "apple")]
        "/usr/lib/libm.dylib",
        #[cfg(windows)]
        "ucrtbase.dll",
    ];
    // SAFETY: only system libraries.
    unsafe { Library::open_first(candidates) }.unwrap();
    // SAFETY: nothing loads.
    let err =
        unsafe { Library::open_first(["libdlsym-missing-1", "libdlsym-missing-2"]) }.unwrap_err();
    match err {
        Error::Open { path, .. } => assert_eq!(path, "libdlsym-missing-2"),
        e => panic!("{e:?}"),
    }
    // SAFETY: nothing to load.
    assert!(unsafe { Library::open_first(Vec::<&str>::new()) }.is_err());
}

#[test]
fn leaked_symbols_are_static() {
    let lib: &'static Library = libc().leak();
    // SAFETY: `strlen` has this signature.
    let strlen: dlsym::Symbol<'static, unsafe extern "C" fn(*const c_char) -> usize> =
        unsafe { lib.get(c"strlen") }.unwrap();
    // SAFETY: the argument is a valid C string.
    assert_eq!(unsafe { strlen.call((c"four".as_ptr(),)) }, 4);
}

#[test]
fn filenames() {
    let name = library_filename("z");
    if cfg!(windows) {
        assert_eq!(name, "z.dll");
    } else if cfg!(target_vendor = "apple") {
        assert_eq!(name, "libz.dylib");
    } else {
        assert_eq!(name, "libz.so");
    }
}
