//! End-to-end tests of the generator: compile `tests/fixtures/demo` (C) and
//! `tests/fixtures/cpp` (C++) into shared libraries with the system
//! compiler, generate bindings for their headers, then build and run each
//! fixture's `main.rs` against them.
//!
//! Skipped (with a message) when libclang or a compiler is unavailable,
//! unless `DLSYM_REQUIRE_E2E` is set, as in CI.

#![cfg(feature = "codegen")]

use std::path::{Path, PathBuf};
use std::process::Command;

use dlsym::codegen::{Builder, parse};

fn skip(why: &str) {
    if std::env::var("DLSYM_REQUIRE_E2E").is_ok_and(|v| !v.is_empty() && v != "0") {
        panic!("end-to-end test required but {why}");
    }
    eprintln!("skipping end-to-end test: {why}");
}

fn compiler(env: &str, names: [&str; 3]) -> Option<String> {
    let candidates = std::env::var(env)
        .into_iter()
        .chain(names.map(String::from));
    candidates.into_iter().find(|cc| {
        Command::new(cc)
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
    })
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/demo")
}

/// Compiles `source` into the shared library `lib`.
fn compile(cc: &str, source: &Path, lib: &Path) {
    let mut cmd = Command::new(cc);
    if cfg!(target_vendor = "apple") {
        cmd.arg("-dynamiclib");
    } else {
        cmd.arg("-shared");
    }
    if cfg!(unix) {
        cmd.arg("-fPIC");
    }
    let status = cmd
        .arg("-o")
        .arg(lib)
        .arg(source)
        .status()
        .expect("run the compiler");
    assert!(status.success(), "{cc} failed on {}", source.display());
}

/// Builds a crate from the generated `code` and `main`, and runs it with
/// `lib` as its argument; it must print `{name} ok`.
fn run(name: &str, work: &Path, code: &str, main: &Path, lib: &Path) {
    std::fs::create_dir_all(work.join("src")).unwrap();
    std::fs::write(work.join("src/bindings.rs"), code).unwrap();
    std::fs::copy(main, work.join("src/main.rs")).unwrap();
    std::fs::write(
        work.join("Cargo.toml"),
        format!(
            "[package]\nname = \"{name}-e2e\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n\
             [dependencies]\ndlsym = {{ path = {:?} }}\n\n[workspace]\n",
            env!("CARGO_MANIFEST_DIR")
        ),
    )
    .unwrap();

    let out = Command::new(env!("CARGO"))
        .args(["run", "--quiet", "--manifest-path"])
        .arg(work.join("Cargo.toml"))
        .arg("--")
        .arg(lib)
        .env(
            "CARGO_TARGET_DIR",
            Path::new(env!("CARGO_TARGET_TMPDIR")).join("e2e-target"),
        )
        // Don't inherit `-D warnings` and the like, but let CI pass flags
        // the nested build needs (musl: `-C target-feature=-crt-static`).
        .env(
            "RUSTFLAGS",
            std::env::var("DLSYM_E2E_RUSTFLAGS").unwrap_or_default(),
        )
        .output()
        .expect("run cargo");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success() && stdout.contains(&format!("{name} ok")),
        "generated bindings failed:\n--- stdout\n{stdout}\n--- stderr\n{stderr}\n--- bindings\n{code}"
    );
}

#[test]
fn demo_library() {
    if cfg!(target_pointer_width = "32") {
        // libclang and the C compiler target the 64-bit host.
        eprintln!("skipping end-to-end test on a 32-bit target");
        return;
    }
    let Some(cc) = compiler("CC", ["cc", "clang", "gcc"]) else {
        return skip("no C compiler found");
    };
    let work = Path::new(env!("CARGO_TARGET_TMPDIR")).join("e2e-demo");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    let lib = work.join(dlsym::library_filename("demo"));
    compile(&cc, &fixtures().join("demo.c"), &lib);

    let generated = match Builder::new()
        .header(fixtures().join("demo.h"))
        .struct_name("Demo")
        .safe("demo_add")
        .safe("demo_is_even")
        .owned("demo_handle", "demo_handle_free", ["demo_handle_new"])
        .safe_handle("demo_handle_get")
        // Probing the built library marks `demo_not_implemented` optional.
        .library_name(lib.to_str().unwrap())
        .generate()
    {
        Ok(g) => g,
        Err(parse::Error::LibClang(e)) => return skip(&e),
        Err(e) => panic!("{e}"),
    };
    assert!(
        generated
            .warnings
            .iter()
            .any(|w| w.contains("demo_long_double")),
        "{:?}",
        generated.warnings
    );
    assert!(
        generated
            .warnings
            .iter()
            .any(|w| w.contains("`demo_not_implemented` is not exported")),
        "{:?}",
        generated.warnings
    );
    let code = generated.code;

    // Check the generated code, then build a crate around it.
    for needle in [
        "#[repr(C, packed)]\n#[derive(Clone, Copy, Debug)]\npub struct demo_packed",
        "#[repr(C, align(16))]",
        "#[repr(C, packed(2))]",
        "pub union demo_number",
        "pub struct demo_handle {\n    _private: [u8; 0],",
        "pub const DEMO_VERSION: &::core::ffi::CStr = c\"1.2.3\";",
        "pub fn demo_sum(count: ::core::ffi::c_int, ...) -> ::core::ffi::c_int;",
        "pub safe fn demo_add(",
        "pub struct DemoHandle<'lib> {",
        "pub fn demo_add_ten(&self, x: ::core::ffi::c_int) -> ::core::ffi::c_int {",
        "#[optional]\n            pub fn demo_not_implemented()",
    ] {
        assert!(code.contains(needle), "missing {needle:?}");
    }
    assert!(
        !code.contains("demo_long_double"),
        "long double is unsupported"
    );
    assert!(
        !code.contains("demo_twice"),
        "only macros wrapping a call are bound"
    );
    assert!(
        !code.contains("demo_inline"),
        "static inline functions have no symbol"
    );
    assert!(!code.contains("DEMO_EMPTY") && !code.contains("DEMO_FUNC_LIKE"));
    run("demo", &work, &code, &fixtures().join("main.rs"), &lib);
}

#[test]
fn cpp_library() {
    if cfg!(target_pointer_width = "32") {
        eprintln!("skipping end-to-end test on a 32-bit target");
        return;
    }
    let Some(cxx) = compiler("CXX", ["c++", "clang++", "g++"]) else {
        return skip("no C++ compiler found");
    };
    let fixtures = fixtures().with_file_name("cpp");
    let work = Path::new(env!("CARGO_TARGET_TMPDIR")).join("e2e-cpp");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    let lib = work.join(dlsym::library_filename("cpp"));
    compile(&cxx, &fixtures.join("cpp.cpp"), &lib);

    let generated = match Builder::new()
        .header(fixtures.join("cpp.hpp"))
        .struct_name("Cpp")
        .library_name(lib.to_str().unwrap())
        .generate()
    {
        Ok(g) => g,
        Err(parse::Error::LibClang(e)) => return skip(&e),
        Err(e) => panic!("{e}"),
    };
    let code = generated.code;
    let warnings = generated.warnings.join("\n");
    assert!(
        warnings.contains("non-trivial C++ type"),
        "make_name returns a non-trivial type by value:\n{warnings}"
    );
    assert!(
        !warnings.contains("not exported"),
        "every mangled name resolves:\n{warnings}"
    );
    for needle in [
        "unsafe extern \"C-unwind\"",
        "pub fn geo_checked_div(",
        "pub fn geo_name_len(n: *const geo_Name)",
        "pub fn cpp_plain(",
        "pub const geo_shape_square: geo_shape",
        // Private members are laid out but not `pub`.
        "pub struct geo_Registry {\n    v: ::core::ffi::c_int,",
        // Non-trivial classes are opaque.
        "pub struct geo_Name {\n    _private: [u8; 0],",
    ] {
        assert!(code.contains(needle), "missing {needle:?} in\n{code}");
    }
    for absent in [
        "secret",
        "fn geo_Registry_value",
        "make_name",
        "fn geo_Registry_Registry",
    ] {
        assert!(
            !code.contains(absent),
            "{absent:?} must not be bound:\n{code}"
        );
    }
    assert!(
        !code.contains("#[link_name = \"cpp_plain\"]"),
        "extern \"C\" functions keep their plain names"
    );
    run("cpp", &work, &code, &fixtures.join("main.rs"), &lib);
}
