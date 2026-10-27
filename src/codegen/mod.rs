//! Generates [`library!`](crate::library!) bindings from C headers.
//!
//! Requires the `codegen` feature. This is the engine behind the `dlsym-gen`
//! binary, and is also usable from a `build.rs` through [`Builder`]:
//!
//! ```no_run
//! // build.rs
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let out = std::path::PathBuf::from(std::env::var("OUT_DIR")?);
//! dlsym::codegen::Builder::new()
//!     .header("/usr/include/zlib.h")
//!     .struct_name("Zlib")
//!     .library_name("libz.so.1")
//!     .generate()?
//!     .write_to(out.join("zlib.rs"))?;
//! println!("cargo::rerun-if-changed=build.rs");
//! # Ok(()) }
//! ```
//!
//! ```ignore
//! // src/lib.rs
//! #[allow(non_camel_case_types, non_snake_case, non_upper_case_globals, dead_code)]
//! mod zlib {
//!     include!(concat!(env!("OUT_DIR"), "/zlib.rs"));
//! }
//! ```
//!
//! The pipeline has three stages:
//!
//! 1. [`parse`]: a C front end, built on libclang loaded at run time (no
//!    LLVM needed at build time), turns headers into the [`ir`].
//! 2. [`ir`]: a small, language-neutral model of the declarations a header
//!    exposes (functions, types, constants).
//! 3. [`emit`]: turns the IR into Rust source built on
//!    [`library!`](crate::library!).

use std::path::{Path, PathBuf};

mod clang;
pub mod emit;
pub mod ir;
pub mod parse;

/// Configures and runs the generator in one go.
///
/// Output from [`Builder::generate`] has no module-level attributes, so it
/// can be pulled in with `include!`; put
/// [`emit::ALLOW_ATTRIBUTES`] on the surrounding module.
#[derive(Debug, Clone, Default)]
pub struct Builder {
    parse: parse::Options,
    emit: emit::Options,
    probe: bool,
}

/// Generated bindings.
#[derive(Debug, Clone)]
pub struct Generated {
    /// The Rust source.
    pub code: String,
    /// Non-fatal problems (skipped items, clang warnings).
    pub warnings: Vec<String>,
}

impl Generated {
    /// Writes the code to `path`.
    pub fn write_to(&self, path: impl AsRef<Path>) -> std::io::Result<()> {
        std::fs::write(path, &self.code)
    }
}

impl Builder {
    /// Creates a builder with default options. The struct is named
    /// `Bindings` unless [`struct_name`](Builder::struct_name) is called.
    pub fn new() -> Self {
        let mut b = Self::default();
        b.emit.struct_name = "Bindings".into();
        b.emit.no_module_attributes = true;
        b.probe = true;
        b
    }

    /// Whether to [`probe`] the library named by
    /// [`library_name`](Builder::library_name) (default: yes).
    pub fn probe(mut self, probe: bool) -> Self {
        self.probe = probe;
        self
    }

    /// Adds a header to parse.
    pub fn header(mut self, path: impl Into<PathBuf>) -> Self {
        self.parse.headers.push(path.into());
        self
    }

    /// Adds a clang argument (`-I`, `-D`, `--target=`, ...).
    pub fn clang_arg(mut self, arg: impl Into<String>) -> Self {
        self.parse.clang_args.push(arg.into());
        self
    }

    /// Also binds declarations from headers under `dir`.
    pub fn header_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.parse.header_dirs.push(dir.into());
        self
    }

    /// Only binds items whose C name matches `glob` (`*`, `?`); repeatable.
    pub fn allow(mut self, glob: impl Into<String>) -> Self {
        self.parse.allow.push(glob.into());
        self
    }

    /// Never binds items whose C name matches `glob`; repeatable.
    pub fn block(mut self, glob: impl Into<String>) -> Self {
        self.parse.block.push(glob.into());
        self
    }

    /// Uses the libclang at `path` instead of searching for one.
    pub fn libclang(mut self, path: impl Into<PathBuf>) -> Self {
        self.parse.libclang = Some(path.into());
        self
    }

    /// Generates for this Rust target triple instead of the host.
    pub fn target(mut self, triple: impl Into<String>) -> Self {
        self.parse.target = Some(triple.into());
        self
    }

    /// Parses the headers as C++ (automatic for `.hpp`, `.hh`, `.hxx` and
    /// `.h++`). See [`parse`] for what C++ is supported.
    pub fn cpp(mut self, cpp: bool) -> Self {
        self.parse.cpp = cpp;
        self
    }

    /// Generates bindings even if clang reports errors.
    pub fn keep_going(mut self, keep_going: bool) -> Self {
        self.parse.keep_going = keep_going;
        self
    }

    /// Names the generated struct.
    pub fn struct_name(mut self, name: impl Into<String>) -> Self {
        self.emit.struct_name = name.into();
        self
    }

    /// Adds a library file name for the generated `open_default`.
    pub fn library_name(mut self, name: impl Into<String>) -> Self {
        self.emit.library_names.push(name.into());
        self
    }

    /// Marks matching functions and variables `#[optional]`.
    pub fn optional(mut self, glob: impl Into<String>) -> Self {
        self.emit.optional.push(glob.into());
        self
    }

    /// Wraps a handle type in an RAII owner released by `free`, with
    /// `{constructor}_owned` methods; see [`emit::Owned`].
    pub fn owned<I, S>(
        mut self,
        type_name: impl Into<String>,
        free: impl Into<String>,
        constructors: I,
    ) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.emit.owned.push(emit::Owned {
            type_name: type_name.into(),
            free: free.into(),
            constructors: constructors.into_iter().map(Into::into).collect(),
        });
        self
    }

    /// Declares matching functions `safe fn`. Only for functions that are
    /// sound to call with any arguments.
    pub fn safe(mut self, glob: impl Into<String>) -> Self {
        self.emit.safe.push(glob.into());
        self
    }

    /// Makes the [`owned`](Self::owned) handle methods of matching functions
    /// safe, leaving the raw functions `unsafe`. Only for functions that are
    /// sound with any live handle and any values of their other arguments.
    pub fn safe_handle(mut self, glob: impl Into<String>) -> Self {
        self.emit.safe_handle.push(glob.into());
        self
    }

    /// Adds typed wrappers for the functions matching a
    /// [`Wrap`](emit::Wrap) spec (`"GLOB:ANNOTATION,..."`, e.g.
    /// `"gzopen:str"`).
    ///
    /// # Panics
    ///
    /// If `spec` is malformed (this is meant for `build.rs`, where a panic
    /// is the build error).
    pub fn wrap(mut self, spec: &str) -> Self {
        match emit::Wrap::parse(spec) {
            Ok(w) => self.emit.wraps.push(w),
            Err(e) => panic!("invalid wrap spec: {e}"),
        }
        self
    }

    /// Keeps (`true`) or drops (`false`, the default for the builder) the
    /// module-level `#![allow(..)]` attributes.
    pub fn module_attributes(mut self, keep: bool) -> Self {
        self.emit.no_module_attributes = !keep;
        self
    }

    /// Parses the headers and generates the bindings.
    pub fn generate(&self) -> Result<Generated, parse::Error> {
        let mut parsed = parse::parse(&self.parse)?;
        let mut emit = self.emit.clone();
        if self.probe {
            let (_, missing, warnings) = probe(&parsed.header, &emit.library_names);
            parsed.warnings.extend(warnings);
            emit.optional.extend(missing);
        }
        emit.comment.push(format!(
            "Generator: dlsym {} ({})",
            env!("CARGO_PKG_VERSION"),
            parsed.clang_version
        ));
        emit.comment.push(format!(
            "Target: {} (struct layouts are checked at compile time)",
            parsed.target.as_deref().unwrap_or("host")
        ));
        Ok(Generated {
            code: emit::emit(&parsed.header, &emit),
            warnings: parsed.warnings,
        })
    }
}

/// Opens the first loadable library in `names` and checks which functions
/// and variables of `header` it does not export.
///
/// Headers often declare functions that only some builds of a library
/// provide (platform-specific or optional features). Marking those
/// `#[optional]` keeps the generated bindings loadable everywhere.
///
/// Returns the library that was opened (if any), the missing symbol names,
/// and a warning for each. The library is kept loaded for the rest of the
/// process, since some libraries are not safe to unload.
pub fn probe(header: &ir::Header, names: &[String]) -> (Option<String>, Vec<String>, Vec<String>) {
    let mut missing = Vec::new();
    let mut warnings = Vec::new();
    let Some((name, lib)) = names.iter().find_map(|n| {
        // SAFETY: the user named this library as the one being bound.
        unsafe { crate::Library::open(n) }
            .ok()
            .map(|l| (n.clone(), l.leak()))
    }) else {
        return (None, missing, warnings);
    };
    for item in &header.items {
        let (rust, symbol) = match item {
            ir::Item::Function(f) => (&f.name, f.symbol()),
            ir::Item::Var(v) => (&v.name, v.symbol()),
            _ => continue,
        };
        // SAFETY: only the address is looked up; nothing is called.
        let found = unsafe { lib.get::<*const u8>(symbol) };
        if let Err(crate::Error::SymbolNotFound { .. }) = found {
            warnings.push(format!(
                "`{rust}` is not exported by {name}; marked #[optional]"
            ));
            missing.push(rust.clone());
        }
    }
    (Some(name), missing, warnings)
}
