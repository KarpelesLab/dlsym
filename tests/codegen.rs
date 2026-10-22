//! End-to-end test of the generator: compiles `tests/fixtures/demo` into a
//! shared library with the system C compiler, generates bindings for
//! `demo.h`, then builds and runs `tests/fixtures/demo/main.rs` against it.
//!
//! Skipped (with a message) when libclang or a C compiler is unavailable,
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

fn compiler() -> Option<String> {
    let candidates = std::env::var("CC")
        .into_iter()
        .chain(["cc", "clang", "gcc"].map(String::from));
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

#[test]
fn demo_library() {
    if cfg!(target_pointer_width = "32") {
        // libclang and the C compiler target the 64-bit host.
        eprintln!("skipping end-to-end test on a 32-bit target");
        return;
    }
    let Some(cc) = compiler() else {
        return skip("no C compiler found");
    };
    let work = Path::new(env!("CARGO_TARGET_TMPDIR")).join("e2e-demo");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(work.join("src")).unwrap();

    // Build the C library.
    let lib = work.join(dlsym::library_filename("demo"));
    let mut cmd = Command::new(&cc);
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
        .arg(&lib)
        .arg(fixtures().join("demo.c"))
        .status()
        .expect("run C compiler");
    assert!(status.success(), "C compiler failed");

    let generated = match Builder::new()
        .header(fixtures().join("demo.h"))
        .struct_name("Demo")
        .safe("demo_add")
        .safe("demo_is_even")
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
        "#[optional]\n            pub fn demo_not_implemented()",
    ] {
        assert!(code.contains(needle), "missing {needle:?}");
    }
    assert!(
        !code.contains("demo_long_double"),
        "long double is unsupported"
    );
    assert!(
        !code.contains("demo_inline"),
        "static inline functions have no symbol"
    );
    assert!(!code.contains("DEMO_EMPTY") && !code.contains("DEMO_FUNC_LIKE"));
    std::fs::write(work.join("src/bindings.rs"), &code).unwrap();
    std::fs::copy(fixtures().join("main.rs"), work.join("src/main.rs")).unwrap();
    std::fs::write(
        work.join("Cargo.toml"),
        format!(
            "[package]\nname = \"demo-e2e\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n\
             [dependencies]\ndlsym = {{ path = {:?} }}\n\n[workspace]\n",
            env!("CARGO_MANIFEST_DIR")
        ),
    )
    .unwrap();

    let out = Command::new(env!("CARGO"))
        .args(["run", "--quiet", "--manifest-path"])
        .arg(work.join("Cargo.toml"))
        .arg("--")
        .arg(&lib)
        .env(
            "CARGO_TARGET_DIR",
            Path::new(env!("CARGO_TARGET_TMPDIR")).join("e2e-target"),
        )
        .env_remove("RUSTFLAGS")
        .output()
        .expect("run cargo");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success() && stdout.contains("demo ok"),
        "generated bindings failed:\n--- stdout\n{stdout}\n--- stderr\n{stderr}\n--- bindings\n{code}"
    );
}
