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

```rust
use dlsym::{Bindings, Library};

// One-off lookups: the symbol borrows the library.
let libm = unsafe { Library::open("libm.so.6")? };
let cos = unsafe { libm.get::<unsafe extern "C" fn(f64) -> f64>(c"cos")? };
assert_eq!(unsafe { cos.call((0.0,)) }, 1.0);

// Whole APIs: a struct that owns the library and every resolved function.
dlsym::library! {
    pub struct LibM {
        pub fn sqrt(x: f64) -> f64;
        pub fn pow(x: f64, y: f64) -> f64;
    }
}
let m = unsafe { LibM::open("libm.so.6")? };
assert_eq!(unsafe { m.pow(2.0, 10.0) }, 1024.0);
```

## Compared to libloading

- Symbols don't `Deref` to the raw function pointer, so the pointer can't be
  copied out and outlive the library by accident.
- `ArcSymbol` gives `'static`-friendly symbols without `transmute`.
- Only function pointers and raw data pointers can be requested
  (`get::<u64>` doesn't compile).
- The `library!` macro and the `dlsym-gen` tool generate structs that resolve
  a whole API at load time.

## dlsym-gen

`dlsym-gen` turns C headers into `dlsym::library!` bindings, and eventually
into complete crates with safe wrappers. It's still a work in progress: the
C parser is not implemented yet.

It ships as a binary of this crate behind the `codegen` feature. Install it
with `cargo install dlsym --features codegen`, or download a prebuilt binary
(Linux, macOS universal, Windows; x86_64 and aarch64) from the
[releases page](https://github.com/KarpelesLab/dlsym/releases). The same
engine is available to `build.rs` scripts as `dlsym::codegen`.

```sh
dlsym-gen --struct LibFoo /usr/include/foo.h -o src/foo.rs -- -I/usr/include/foo
```

See [ROADMAP.md](ROADMAP.md).

## License

MIT
