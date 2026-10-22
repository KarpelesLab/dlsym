# dlsym

[![CI](https://github.com/KarpelesLab/dlsym/actions/workflows/ci.yml/badge.svg)](https://github.com/KarpelesLab/dlsym/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/dlsym.svg)](https://crates.io/crates/dlsym)
[![docs.rs](https://img.shields.io/docsrs/dlsym)](https://docs.rs/dlsym)
[![MSRV](https://img.shields.io/badge/MSRV-1.95-orange.svg)](Cargo.toml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Bindings around the platform's dynamic library loading primitives with greatly
improved memory safety. The most important safety guarantee of this library
is the prevention of dangling symbols that may occur after a library is
unloaded.

It comes with **`dlsym-gen`**, which turns C headers into Rust bindings (or a
whole crate) that load the library at run time: no link-time dependency, no
`-sys` crate, and no LLVM needed to build.

## Loading symbols

```rust
use dlsym::Library;

// SAFETY: libm's initialisers are trusted.
let libm = unsafe { Library::open("libm.so.6")? };
// SAFETY: `cos` has this signature.
let cos = unsafe { libm.get::<unsafe extern "C" fn(f64) -> f64>(c"cos")? };
// SAFETY: `cos` is defined for every `f64`.
assert_eq!(unsafe { cos.call((0.0,)) }, 1.0);
// drop(libm) here would not compile: `cos` borrows it.
```

- `Symbol<'lib, T>` borrows its `Library`, so the borrow checker rejects
  unloading while a symbol is in use.
- Symbols don't `Deref` to the raw pointer. Functions are called with
  `call((args,))`, variadic ones with `with(|f| f(..))`, and the raw pointer
  only comes out through `unsafe fn into_inner`.
- `ArcSymbol<T>` (from `Library::get_arc`) keeps the library alive through an
  `Arc`, and `Library::leak` gives `'static` symbols, both without
  `transmute`.
- Only `unsafe extern` function pointers and raw data pointers can be
  requested: `get::<u64>` and safe `extern "C" fn` types don't compile.
- A symbol whose address is null (`NullSymbol`) is told apart from a missing
  one (`SymbolNotFound`).
- `Library::open_first([...])`, `Library::this()`, `library_filename("z")`,
  plus `os::unix::OpenOptions` (`RTLD_LAZY`/`GLOBAL`/`NODELETE`/`NOLOAD`/
  `DEEPBIND`, `dlvsym`) and `os::windows::OpenOptions`
  (`LOAD_LIBRARY_SEARCH_*`, already-loaded modules; loader error dialogs are
  suppressed).

## Binding a whole API

`library!` declares a struct that owns the library and every symbol, using
the same syntax as a Rust 2024 `unsafe extern` block:

```rust
use std::ffi::{c_char, c_int};
use dlsym::Bindings;

dlsym::library! {
    pub struct LibC {
        unsafe extern "C" {
            pub safe fn abs(x: c_int) -> c_int;              // callable without `unsafe`
            pub fn strlen(s: *const c_char) -> usize;        // an `unsafe` method
            pub fn printf(fmt: *const c_char, ...) -> c_int; // variadic
            #[optional]                                      // may be missing
            pub fn strlcpy(d: *mut c_char, s: *const c_char, n: usize) -> usize;
            #[link_name = "strlen"]                          // renamed
            pub fn len(s: *const c_char) -> usize;
            pub static mut environ: *const *const c_char;    // data
        }
    }
}

// SAFETY: libc's initialisers are trusted.
let libc = unsafe { LibC::open("libc.so.6")? };
assert_eq!(libc.abs(-4), 4);
assert_eq!(unsafe { libc.strlen(c"abc".as_ptr()) }, 3);
unsafe { libc.printf().with(|f| f(c"%d\n".as_ptr(), 42)) };
```

Every non-`#[optional]` symbol is resolved when the struct is created, so a
call is a constant-index load plus the call itself. The function pointers
live inside the struct next to the library, so they cannot outlive it.

## Generating bindings from C headers

Install the generator with `cargo install dlsym --features codegen`, or
download a prebuilt `dlsym-gen` (Linux, macOS universal and Windows, for
x86_64 and aarch64) from the
[releases page](https://github.com/KarpelesLab/dlsym/releases). It needs a
libclang at run time: one from LLVM, Xcode, or your distribution's
`libclang` package. It is found through `--libclang`, `$LIBCLANG_PATH`,
`llvm-config`, or the usual install locations.

```sh
# A crate (Cargo.toml and lib.rs are only written if missing):
dlsym-gen /usr/include/zlib.h --crate zlib-dl -l libz.so.1 -l libz.1.dylib -l zlib1.dll

# Or a single module:
dlsym-gen /usr/include/foo/foo.h -o src/foo.rs --struct Foo -- -I/usr/include/foo
```

```rust
use zlib_dl::*;

let z = unsafe { Zlib::open_default() }?;   // tries each -l name in turn
let mut strm = z_stream::default();         // zeroed, like C's `= {0}`
let version = ZLIB_VERSION.as_ptr();        // #defines become constants
let rc = unsafe { z.deflateInit_(&mut strm, Z_BEST_COMPRESSION, version, size_of::<z_stream>() as i32) };
```

What it handles:

- Functions (variadic, calling conventions), global variables, structs and
  unions (packed, aligned, `#pragma pack`, bitfields with typed accessors,
  C11 anonymous members, flexible array members), enums (as newtypes, so an
  unexpected value from C is never undefined behaviour), typedefs and
  function pointers.
- Object-like `#define`s, evaluated by clang: integers, floats, strings and
  constant expressions.
- Struct layouts are checked at compile time against the C compiler's
  (`size_of`, `align_of`, `offset_of!`).
- It binds what the given headers declare, plus the headers they include
  with `#include "..."` (`--header-dir` adds more). Types from elsewhere are
  pulled in as needed: in full when used by value, opaque when only used
  behind pointers.
- Headers often declare functions that only some builds of a library export
  (sqlite's Windows-only functions, optional features). With `-l`, the
  generator opens the installed library and marks whatever it lacks
  `#[optional]`, so the bindings still load; calling such a function returns
  `None`.
- `--allow`/`--block` filter by name, `--optional` marks more symbols that
  may be missing, and `--safe` declares functions callable without
  `unsafe`. Unsupported items (e.g. `long double`) are skipped with a
  warning.
- Tested on zlib, sqlite3, curl, libpng, xz, bzip2, zstd, expat, libarchive,
  libmagic, ogg, opus, libsodium, libuv, libyaml, lz4, pcre2, SDL2, Xlib,
  OpenGL, EGL, Vulkan, OpenSSL, libxml2, GLib, D-Bus, cairo, FreeType, ALSA,
  PulseAudio, ncurses, readline, libpcap, libusb, libudev, libevent,
  libjpeg, libtiff, libwebp, fontconfig, gdbm and libuuid: every generated
  module compiles.

From a `build.rs`, use `dlsym::codegen::Builder` (see the
[`codegen` docs](https://docs.rs/dlsym/latest/dlsym/codegen/)).

## Compared to libloading

|                                      | libloading | dlsym |
|--------------------------------------|:----------:|:-----:|
| Symbols borrow their library         | ✓          | ✓     |
| Raw pointer copyable out of a symbol | via `*sym` | only `unsafe` |
| `'static` symbols without transmute  |            | `ArcSymbol`, `leak` |
| Symbol type checked to be a pointer  |            | ✓     |
| Whole-API structs (`library!`)       |            | ✓     |
| Header → bindings generator          |            | ✓     |

See [ROADMAP.md](ROADMAP.md) for what's next.

## License

MIT
