# Roadmap

A single crate, `dlsym`:

| Part                          | Role                                                     |
|-------------------------------|----------------------------------------------------------|
| library (default)             | Runtime loader. Only depends on `libc` / `windows-sys`.  |
| `codegen` feature             | `dlsym::codegen`: C header → Rust generator, for `build.rs`. |
| `dlsym-gen` binary (`codegen`) | Command-line front end to `dlsym::codegen`.              |

Generated code only needs the default features, and nothing in the pipeline
needs LLVM at build time. The generator has no dependencies beyond `dlsym`
itself; keep it that way, or gate any new one to the binary.

## Design principles

1. **No dangling symbols through safe code.** `Symbol<'lib, T>` borrows its
   `Library`, `ArcSymbol<T>` owns a share of it, and `library!` structs own it
   outright. Raw pointers only leave through `unsafe fn into_inner`.
2. **No `Deref<Target = T>`.** libloading's `*sym` copies the function
   pointer out, after which nothing prevents it from outliving the library.
   Functions are invoked through `call` instead.
3. **Sealed symbol types.** Only `unsafe extern ".." fn` pointers and raw
   data pointers can be requested. `get::<u64>` or a safe `extern "C" fn`
   type is rejected at compile time.
4. **Null is not "not found".** A symbol may legitimately resolve to null;
   `dlerror` is what distinguishes the two (`NullSymbol` vs `SymbolNotFound`).
5. **Every `unsafe` block carries a `// SAFETY:` comment** (enforced by
   clippy lints in `Cargo.toml`).

## Phase 0: Scaffolding (done)

- [x] Workspace, edition 2024, MSRV 1.95, lints
- [x] `Library::open` / `get` / `get_arc` / `close`, `Drop` unloads
- [x] `Symbol` and `ArcSymbol` with tuple-based `call`
- [x] Sealed `SymbolType` / `FnPtr` (0–12 args; `C`, `C-unwind`, `system`, `system-unwind`)
- [x] Unix backend (`dlopen`), `os::unix::LibraryExt` (`open_with_flags`, `this`)
- [x] Windows backend written, **never compiled**
- [x] `library!` macro and the `Bindings` trait
- [x] `codegen` feature and `dlsym-gen` binary: IR, emitter (functions only), CLI; parser stubbed
- [x] Tests against libm, plus a `compile_fail` test that a symbol can't outlive its library

## Phase 1: Core loader hardening → `dlsym` 0.1

- [x] CI: Linux x86_64, macOS aarch64, Windows x86_64 and i686 (msvc), each
      on stable and MSRV; clippy on all three OSes; docs; rustfmt
- [ ] CI: more targets — Linux aarch64, macOS x86_64, windows-gnu, FreeBSD
      (via a VM action). musl only makes sense dynamically linked:
      statically linked musl binaries cannot `dlopen`
- [ ] Windows: compile and test; `SetThreadErrorMode` so failed loads don't
      pop up dialogs; `LOAD_LIBRARY_SEARCH_*` flags; `Library::this()` via
      `GetModuleHandleExW`
- [ ] `OpenOptions` builder in place of raw `c_int` flags
      (`RTLD_NODELETE`, `RTLD_NOLOAD`, `RTLD_DEEPBIND`)
- [ ] glibc `dlvsym` for versioned symbols
- [ ] `dladdr` / `dlinfo`: find the path of a loaded library
- [ ] Data symbols: typed `as_ref` / `as_mut` accessors tied to `'lib`;
      document thread-local (`__thread`) symbol caveats
- [ ] Variadic function pointers (`unsafe extern "C" fn(A, ...)`), as a
      `SymbolType` that is only callable through `into_inner`
- [ ] Name helpers: `lib{name}.so.N`, `lib{name}.dylib`, `{name}.dll`;
      `Library::open_first(&[candidates])`
- [ ] Unload safety docs: TLS destructors, `atexit` handlers and threads
      started by a library are unsound to unload; offer `Library::leak()`
- [ ] Audit `dlerror` thread-safety on every supported Unix
- [ ] **Open question:** should `get` be safe? Every use of a sealed symbol
      type already requires `unsafe`, so its safety contract could move
      there.

## Phase 2: `library!` macro

- [ ] `safe fn` items, asserted by the invocation, mirroring edition 2024
      `unsafe extern` blocks
- [ ] Optional symbols (`#[optional] fn foo()`): methods return `Option`,
      and loading doesn't fail when they're missing
- [ ] `#[link_name = ".."]`, per-block ABI (`extern "system"`), versioned names
- [ ] Data items (`static VERSION: *const c_char;`)
- [ ] Optional lazy resolution (resolve on first call through a `OnceLock`)
- [ ] Function names that are Rust keywords (`stringify!(r#type)` gives `"r#type"`)
- [ ] Stay declarative for as long as possible, and only move to a proc
      macro if diagnostics demand it

## Phase 3: `dlsym-gen` C front end

- [ ] Minimal libclang bindings written with `dlsym::library!` and loaded
      at runtime, so `dlsym` is used to build `dlsym-gen` itself.
      Discovery via `LIBCLANG_PATH`, `llvm-config`, and well-known paths
- [ ] Only emit declarations from the requested header(s), not transitive
      system includes; allowlist and blocklist by name or regex
- [ ] IR and emitter for:
  - [ ] typedefs
  - [ ] opaque structs (zero-sized `#[repr(C)]` + `PhantomData<(*mut u8, PhantomPinned)>`)
  - [ ] complete structs and unions, laid out from clang's size, alignment
        and offsets, with compile-time layout assertions
  - [ ] enums as newtype constants, not Rust enums (an out-of-range value
        from C would be UB)
  - [ ] function pointer types, arrays, bitfields
  - [ ] `#define` integer and string constants, evaluated by clang
- [ ] Doc comment forwarding
- [ ] Record the target triple and clang version in the generated header,
      and warn about platform-dependent layouts

## Phase 4: Safety layer and crate generation

The step from *bindings* to *safe bindings*:

- [ ] `dlsym-gen.toml` annotation file per library:
  - functions that are safe to call
  - pointer nullability; `const char *` ↔ `&CStr`
  - `(ptr, len)` pairs ↔ slices
  - ownership, e.g. "`foo_new` returns an object freed by `foo_free`",
    generating an RAII wrapper whose `Drop` calls the free function and
    which borrows the `library!` struct
  - errors, e.g. "negative return means `errno`", generating `Result`
- [ ] `dlsym-gen crate <header> --name libfoo-dl`: emits a complete crate
      (`Cargo.toml`, raw `sys` module, safe wrapper module, README)
- [ ] `build.rs` API (`dlsym::codegen::Builder`)
- [ ] Possibly ship annotation sets for common system libraries

## Phase 5: Release

- [ ] docs.rs metadata covering Unix and Windows targets
- [ ] Examples (`examples/`) for each feature
- [x] release-plz: versioning, changelog and crates.io publishing on
      `master`, with prebuilt `dlsym-gen` binaries attached to each release
- [ ] 0.1.0 once phases 1–3 land (0.0.x pre-releases until then)
