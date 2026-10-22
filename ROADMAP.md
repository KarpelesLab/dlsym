# Roadmap

A single crate, `dlsym`:

| Part                           | Role                                                          |
|--------------------------------|---------------------------------------------------------------|
| library (default)              | Runtime loader. Only depends on `libc` / `windows-sys`.       |
| `codegen` feature              | `dlsym::codegen`: C header → Rust generator, for `build.rs`.  |
| `dlsym-gen` binary (`codegen`) | Command-line front end to `dlsym::codegen`.                   |

Generated code only needs the default features, and nothing in the pipeline
needs LLVM at build time: libclang is loaded at run time through `dlsym`
itself. The generator has no dependencies beyond `dlsym`; keep it that way,
or gate any new one to the binary.

## Design principles

1. **No dangling symbols through safe code.** `Symbol<'lib, T>` borrows its
   `Library`, `ArcSymbol<T>` owns a share of it, and `library!` structs own it
   outright. Raw pointers only leave through `unsafe` methods.
2. **No `Deref<Target = T>`.** libloading's `*sym` copies the function
   pointer out, after which nothing prevents it from outliving the library.
   Functions are invoked through `call` (or `with` for variadics) instead.
3. **Sealed symbol types.** Only `unsafe extern ".." fn` pointers and raw
   data pointers can be requested. `get::<u64>` or a safe `extern "C" fn`
   type is rejected at compile time.
4. **Null is not "not found".** A symbol may legitimately resolve to null;
   `dlerror` is what distinguishes the two (`NullSymbol` vs `SymbolNotFound`).
5. **Generated code is checked, not trusted.** Every generated struct
   asserts its size, alignment and field offsets at compile time; enums are
   newtypes so no C value is undefined behaviour.
6. **Every `unsafe` block carries a `// SAFETY:` comment** (enforced by
   clippy lints in `Cargo.toml`).

## Done

### Loader

- [x] `Library::open` / `open_first` / `this` / `get` / `get_arc` / `leak` /
      `close`; `library_filename`
- [x] `Symbol` and `ArcSymbol`: `call`, `with`, data `as_ref` / `as_mut`
- [x] Sealed `SymbolType` / `FnPtr`: 0–12 args over `C`, `C-unwind`,
      `system`, `system-unwind`; variadic `C`/`C-unwind` with 1–6 fixed args
- [x] Unix: `OpenOptions` (lazy, global, no_delete, no_load, deep_bind),
      `LibraryExt::get_versioned` (`dlvsym`)
- [x] Windows: `OpenOptions` (`LOAD_LIBRARY_SEARCH_*`),
      `LibraryExt::open_already_loaded`, error dialogs suppressed while loading
- [x] CI: Linux x86_64, macOS aarch64, Windows x86_64 and i686 (msvc), each
      on stable and MSRV; clippy on all three OSes; docs; rustfmt

### `library!`

- [x] Rust 2024 `unsafe extern "ABI" { .. }` syntax: `safe fn`, `static`,
      `safe static`, `static mut`, variadics, `#[optional]`, `#[link_name]`,
      raw identifiers
- [x] Linear expansion with no recursion limit across items (symbols in a
      fixed array, slots found by a compile-time binary search)

### Generator

- [x] libclang bindings written with `library!`, loaded at run time;
      discovery via `--libclang`, `LIBCLANG_PATH`, `llvm-config`, and
      well-known paths on Linux, macOS and Windows
- [x] Selection: input headers, their `#include "..."`s, `--header-dir`;
      `--allow` / `--block` globs; dependency closure (complete when used
      by value, opaque otherwise)
- [x] Functions, globals, typedefs, opaque and complete structs/unions
      (padding, `packed(N)`, `align(N)`, bitfield accessors, anonymous
      members, flexible arrays), newtype enums, function pointers, arrays,
      calling conventions, `#define` constants, doc comments
- [x] `--optional`, `--safe`, `-l` library names (`open_default`)
- [x] `--crate` output, `dlsym::codegen::Builder` for `build.rs`
- [x] End-to-end test: compile a C fixture, generate, build and run

## Next

### Loader

- [ ] CI: more targets: Linux aarch64, macOS x86_64, windows-gnu, FreeBSD
      (via a VM action). musl only makes sense dynamically linked:
      statically linked musl binaries cannot `dlopen`
- [ ] `dladdr` / `dlinfo`: find the path of a loaded library or symbol
- [ ] Unload safety docs: TLS destructors, `atexit` handlers and threads
      started by a library are unsound to unload (`leak` / `no_delete`)
- [ ] Document thread-local (`__thread`) data symbol caveats
- [ ] Audit `dlerror` thread-safety on every supported Unix
- [ ] **Open question:** should `get` be safe? Every use of a sealed symbol
      type already requires `unsafe`, so its safety contract could move
      there.

### `library!`

- [ ] Optional lazy resolution (resolve on first call through a `OnceLock`)
- [ ] Per-item `#[cfg]` (today: put the whole invocation under `cfg`, or use
      `#[optional]`)
- [ ] Faster expansion for very large APIs (2000 items take ~11 s to compile)

### Generator

- [ ] Big-endian bitfield layouts are implemented but untested
- [ ] Function-like macros that wrap a single call (e.g. zlib's
      `deflateInit`) as helper methods
- [ ] Cross-generation: map `--target` to clang's target triple, and record
      it in the output
- [ ] Generate for several targets and merge them with `#[cfg]`

### Safety layer

The step from *bindings* to *safe bindings*:

- [ ] `dlsym-gen.toml` annotation file per library:
  - pointer nullability; `const char *` ↔ `&CStr`
  - `(ptr, len)` pairs ↔ slices
  - ownership, e.g. "`foo_new` returns an object freed by `foo_free`",
    generating an RAII wrapper whose `Drop` calls the free function and
    which borrows the `library!` struct
  - errors, e.g. "negative return means `errno`", generating `Result`
- [ ] Possibly ship annotation sets for common system libraries

### Release

- [x] release-plz: versioning, changelog and crates.io publishing on
      `master`, with prebuilt `dlsym-gen` binaries attached to each release
- [x] Examples: `symbols` (borrowed symbols, variadics) and `bindings` (`library!`)
- [ ] 0.1.0 once the safety layer's shape is settled (0.0.x until then)
