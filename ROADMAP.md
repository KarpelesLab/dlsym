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

- [x] CI: Linux aarch64, macOS x86_64, windows-gnu, FreeBSD (VM)
- [x] CI: musl (dynamically linked only: statically linked musl binaries
      cannot `dlopen`)
- [x] `Library::path`, `AddressInfo::of`, `Symbol::info` (`dlinfo`, dyld,
      `dladdr`, `GetModuleFileNameW`)
- [x] Platform notes: unload safety, thread-local data symbols, `dlerror`
      thread-safety, lazy binding
- [ ] **Open question:** should `get` be safe? Every use of a sealed symbol
      type already requires `unsafe`, so its safety contract could move
      there.

### `library!`

- [-] Lazy resolution: not planned. Resolving upfront costs ~0.1 µs per
      symbol (0.2 ms for 2000), missing symbols are handled by `#[optional]`
      and generation-time probing, and lazy resolution would add a check to
      every call and move errors from load time to call time
- [x] Per-item `#[cfg]`: disabled items are neither looked up nor bound
- [x] Faster expansion: items are matched per block (rustc's matcher is
      quadratic in pattern input size) and slots are found by a sorted-hash
      search; 2000 items in blocks of 64 compile in ~5 s, OpenSSL's bindings
      in ~3 s

### Generator

- [x] Big-endian: a weekly s390x (QEMU) workflow runs the full suite,
      including the end-to-end bitfield checks
- [x] Function-like macros that wrap a single call (e.g. zlib's
      `deflateInit`) as helper methods
- [x] Cross-generation: `--target` (mapped to clang's triple), recorded in
      the output; layout assertions reject bindings built for another target
- [ ] Generate for several targets and merge them with `#[cfg]`

### C++

Symbols are found through the mangled names clang computes from the
headers; nothing is ever demangled (mangled names do not carry enough type
information to generate checked bindings).

- [x] `--cpp` (automatic for `.hpp`/`.hh`/`.hxx`): functions and variables
      in namespaces, public static members, `#[link_name]` with the target's
      mangling (Itanium or MSVC), `_`-qualified names, overloads suffixed by
      parameter types, references as pointers, `C-unwind` unless
      `noexcept`, `enum class`, template specialization names; non-trivial
      types are opaque and never passed by value; inline functions skipped
- [x] Non-virtual member functions (`this` first, `*const` for `const`
      methods; `thiscall` on 32-bit Windows, now supported by the loader
      along with `stdcall` and `fastcall`)
- [x] Constructors (complete-object, except copy/move and abstract classes)
      and destructors; classes with both get an RAII wrapper that allocates
      the object, constructs it in place, never moves it, and destroys and
      frees it on drop; methods become wrapper methods
- [ ] Virtual functions (through the vtable, not the symbol)
- [ ] Trivially copyable template specializations by value (libclang does
      not list their implicitly instantiated members, so all non-POD
      specializations are currently treated as non-trivial)

### Safety layer

The step from *bindings* to *safe bindings*:

- [x] Ownership: `--owned TYPE=FREE,CTOR...` generates an RAII wrapper
      whose `Drop` calls the free function and which borrows the
      `library!` struct, plus `CTOR_owned` constructors
- [x] Methods on owned handles: `gzwrite(f, ..)` → `f.gzwrite(..)`,
      `demo_handle_get(h)` → `h.get()`; `&self` for `const T *`, `&mut self`
      otherwise; `--safe-handle` makes a method safe without making the raw
      function safe
- [x] Annotations stay CLI/`Builder` options (no new file format); long
      lists go in `@file` argument files
- [x] Typed wrappers, `--wrap 'GLOB:ANNOTATIONS'`: `str` (`&CStr`),
      `slice=PTR/LEN`, `out=NAME`, `ret=nonzero|negative|errno|static-str`,
      `safe`. Library functions get a suffixed `NAME_rs` method next to the
      raw one; handle methods and constructors apply them directly
- [ ] Nullable strings and pointers (`Option<&CStr>`), owned return values
      (`Result<GzFile, _>` from an out-parameter)
- [ ] Possibly ship annotation sets for common system libraries

### Release

- [x] release-plz: versioning, changelog and crates.io publishing on
      `master`, with prebuilt `dlsym-gen` binaries attached to each release
- [x] Examples: `symbols` (borrowed symbols, variadics) and `bindings` (`library!`)
- [ ] 0.1.0 once the safety layer's shape is settled (0.0.x until then)
