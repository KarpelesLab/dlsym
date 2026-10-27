# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.0.3](https://github.com/KarpelesLab/dlsym/compare/v0.0.2...v0.0.3) - 2026-10-27

### Added

- per-item #[cfg] in library!
- *(codegen)* RAII wrappers for owned handles
- *(codegen)* --target for cross-generation
- *(codegen)* bind function-like macros that wrap a call
- Library::path, AddressInfo and Symbol::info

### Fixed

- fix clippy on Windows: one unsafe operation per block in errno_error
- build on macOS with current libc; FreeBSD test fixes

### Other

- reflow
- nullable strings (str?) and owned out-parameters
- rustfmt
- typed wrappers (--wrap): &CStr, slices, out-parameters, Result
- export the C++ fixture's Name class on Windows
- use MSVC's base-object destructor symbol (??1)
- C++ member functions, constructors, destructors and RAII objects
- spell C++ operator functions as identifiers (operator<< -> operator_shl)
- C++ headers through clang's mangled names
- methods on owned handles, --safe-handle, @file argument files
- big-endian bitfields verified on s390x
- weekly big-endian (s390x) test run
- lazy resolution is not planned
- faster library! expansion for large APIs
- test on musl (Alpine, dynamically linked)

## [0.0.2](https://github.com/KarpelesLab/dlsym/compare/v0.0.1...v0.0.2) - 2026-10-22

### Added

- *(codegen)* probe the library for missing symbols; follow in-library includes
- *(codegen)* libclang front end, full emitter and crate generation
- [**breaking**] loader extensions and extern-block syntax for library!

### Fixed

- *(codegen)* never unload libclang; add Builder and --safe

### Other

- add symbols and bindings examples
