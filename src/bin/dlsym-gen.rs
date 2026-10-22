//! `dlsym-gen`: generate runtime-loaded `dlsym` bindings from C headers.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use dlsym::codegen::{emit, parse};

const USAGE: &str = "\
usage: dlsym-gen [options] <header.h>... [-- <clang args>...]

Generates Rust bindings that load a C library at run time with dlsym.

Output:
  -o, --output <file>       write the bindings module to <file> (default: stdout)
      --crate <dir>         write a crate to <dir>: Cargo.toml and src/lib.rs
                            (only if missing) and src/bindings.rs (always)
      --crate-name <name>   package name for --crate (default: the directory name)

Bindings:
  -s, --struct <Name>       name of the generated struct (default: from the
                            first header, e.g. zlib.h -> Zlib)
  -l, --lib <file>          library file name tried by the generated
                            `open_default` (repeatable, e.g. -l libz.so.1).
                            The first one that loads is also probed: symbols
                            it does not export become #[optional]
      --no-probe            do not open the library to probe its symbols
      --optional <glob>     mark matching functions and variables #[optional]
                            (repeatable)
      --owned <TYPE=FREE[,CTOR...]>
                            wrap a handle type in an RAII owner released by
                            FREE, with CTOR_owned constructors returning it
                            (repeatable, e.g. --owned gzFile=gzclose,gzopen)
      --safe <glob>         declare matching functions `safe fn` (callable
                            without `unsafe`); only for functions that are
                            sound with any arguments (repeatable)
      --safe-handle <glob>  make the --owned handle methods of matching
                            functions safe (the raw functions stay unsafe);
                            only for functions that are sound with any live
                            handle and any other arguments (repeatable)

Functions taking an --owned handle first become methods of its wrapper:
demo_handle_get(h) -> h.get(), gzwrite(f, ..) -> f.gzwrite(..).

Options can be read from a file with @<file>: arguments separated by
whitespace or newlines, \"double quotes\" for arguments containing spaces, and
# comments.

Selection (declarations from the headers and the files they #include \"...\"):
      --header-dir <dir>    also bind declarations from headers under <dir>
      --allow <glob>        only bind items whose C name matches (repeatable)
      --block <glob>        never bind items whose C name matches (repeatable)

Clang:
      --target <triple>     generate for this Rust target instead of the host
                            (add --sysroot etc. after `--` as needed)
      --libclang <path>     libclang to use (default: $LIBCLANG_PATH,
                            llvm-config, then well-known locations)
      --keep-going          generate even if clang reports errors
  -q, --quiet               do not print warnings
  -h, --help                show this help
  -V, --version             show the version
";

#[derive(Default)]
struct Args {
    headers: Vec<PathBuf>,
    output: Option<PathBuf>,
    crate_dir: Option<PathBuf>,
    crate_name: Option<String>,
    struct_name: Option<String>,
    libs: Vec<String>,
    parse: parse::Options,
    optional: Vec<String>,
    safe: Vec<String>,
    safe_handle: Vec<String>,
    owned: Vec<emit::Owned>,
    no_probe: bool,
    quiet: bool,
}

enum Command {
    Run(Box<Args>),
    Help,
    Version,
}

fn parse_args(mut it: impl Iterator<Item = String>) -> Result<Command, String> {
    let mut a = Args::default();
    while let Some(arg) = it.next() {
        let mut value = |name: &str| it.next().ok_or_else(|| format!("{name} needs a value"));
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "-V" | "--version" => return Ok(Command::Version),
            "-o" | "--output" => a.output = Some(value(&arg)?.into()),
            "--crate" => a.crate_dir = Some(value(&arg)?.into()),
            "--crate-name" => a.crate_name = Some(value(&arg)?),
            "-s" | "--struct" => a.struct_name = Some(value(&arg)?),
            "-l" | "--lib" => a.libs.push(value(&arg)?),
            "--optional" => a.optional.push(value(&arg)?),
            "--safe" => a.safe.push(value(&arg)?),
            "--safe-handle" => a.safe_handle.push(value(&arg)?),
            "--owned" => a.owned.push(emit::Owned::parse(&value(&arg)?)?),
            "--no-probe" => a.no_probe = true,
            "--header-dir" => a.parse.header_dirs.push(value(&arg)?.into()),
            "--allow" => a.parse.allow.push(value(&arg)?),
            "--block" => a.parse.block.push(value(&arg)?),
            "--libclang" => a.parse.libclang = Some(value(&arg)?.into()),
            "--target" => a.parse.target = Some(value(&arg)?),
            "--keep-going" => a.parse.keep_going = true,
            "-q" | "--quiet" => a.quiet = true,
            "--" => a.parse.clang_args.extend(it.by_ref()),
            s if s.starts_with('-') => return Err(format!("unknown option {s}")),
            s => a.headers.push(s.into()),
        }
    }
    if a.headers.is_empty() {
        return Err("missing header path".into());
    }
    if a.output.is_some() && a.crate_dir.is_some() {
        return Err("--output and --crate are mutually exclusive".into());
    }
    a.parse.headers = a.headers.clone();
    Ok(Command::Run(Box::new(a)))
}

/// Replaces each `@file` argument with the arguments read from `file`
/// (recursively, up to a fixed depth).
fn expand_args(args: impl IntoIterator<Item = String>, depth: u32) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for arg in args {
        match arg.strip_prefix('@') {
            Some(path) if !path.is_empty() => {
                if depth >= 8 {
                    return Err(format!("@{path}: argument files nested too deeply"));
                }
                let text = std::fs::read_to_string(path).map_err(|e| format!("@{path}: {e}"))?;
                let words = split_args_file(&text).map_err(|e| format!("@{path}: {e}"))?;
                out.extend(expand_args(words, depth + 1)?);
            }
            _ => out.push(arg),
        }
    }
    Ok(out)
}

/// Splits an argument file: arguments are separated by whitespace,
/// `"double quotes"` group (with `\"` and `\\` escapes), and `#` starts a
/// comment running to the end of the line.
fn split_args_file(text: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else if c == '#' {
            while chars.next().is_some_and(|c| c != '\n') {}
        } else {
            let mut word = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() {
                    break;
                }
                chars.next();
                if c != '"' {
                    word.push(c);
                    continue;
                }
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') if matches!(chars.peek(), Some('"' | '\\')) => {
                            word.extend(chars.next());
                        }
                        Some(c) => word.push(c),
                        None => return Err("unterminated quote".into()),
                    }
                }
            }
            out.push(word);
        }
    }
    Ok(out)
}

/// `zlib.h` -> `Zlib`, `foo-bar_baz.h` -> `FooBarBaz`.
fn struct_name_for(header: &Path) -> String {
    let stem = header
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("bindings");
    let mut out = String::new();
    for part in stem
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|p| !p.is_empty())
    {
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            out.push(first.to_ascii_uppercase());
            out.extend(chars);
        }
    }
    if out.is_empty() || out.starts_with(|c: char| c.is_ascii_digit()) {
        out.insert_str(0, "Lib");
    }
    out
}

fn main() -> ExitCode {
    let args =
        match expand_args(std::env::args().skip(1), 0).and_then(|a| parse_args(a.into_iter())) {
            Ok(Command::Run(a)) => a,
            Ok(Command::Help) => {
                print!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            Ok(Command::Version) => {
                println!("dlsym-gen {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            Err(e) => {
                eprint!("dlsym-gen: {e}\n\n{USAGE}");
                return ExitCode::from(2);
            }
        };
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("dlsym-gen: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &Args) -> Result<(), String> {
    let mut parsed = parse::parse(&args.parse).map_err(|e| e.to_string())?;
    let mut optional = args.optional.clone();
    if !args.no_probe {
        let (_, missing, warnings) = dlsym::codegen::probe(&parsed.header, &args.libs);
        parsed.warnings.extend(warnings);
        optional.extend(missing);
    }
    if !args.quiet {
        for w in &parsed.warnings {
            eprintln!("warning: {w}");
        }
    }
    let struct_name = args
        .struct_name
        .clone()
        .unwrap_or_else(|| struct_name_for(&args.headers[0]));
    let headers: Vec<String> = args
        .headers
        .iter()
        .map(|h| {
            h.file_name()
                .unwrap_or(h.as_os_str())
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    let code = emit::emit(
        &parsed.header,
        &emit::Options {
            struct_name: struct_name.clone(),
            library_names: args.libs.clone(),
            optional,
            safe: args.safe.clone(),
            safe_handle: args.safe_handle.clone(),
            owned: args.owned.clone(),
            no_module_attributes: false,
            comment: vec![
                format!("Source: {}", headers.join(", ")),
                format!(
                    "Generator: dlsym-gen {} ({})",
                    env!("CARGO_PKG_VERSION"),
                    parsed.clang_version
                ),
                format!(
                    "Target: {} (struct layouts are checked at compile time)",
                    parsed.target.as_deref().unwrap_or("host")
                ),
            ],
        },
    );

    if let Some(dir) = &args.crate_dir {
        write_crate(
            dir,
            args.crate_name.as_deref(),
            &struct_name,
            &headers,
            &args.libs,
            &code,
        )
    } else if let Some(path) = &args.output {
        std::fs::write(path, code).map_err(|e| format!("{}: {e}", path.display()))
    } else {
        print!("{code}");
        Ok(())
    }
}

fn write_crate(
    dir: &Path,
    name: Option<&str>,
    struct_name: &str,
    headers: &[String],
    libs: &[String],
    code: &str,
) -> Result<(), String> {
    let io = |p: &Path, e: std::io::Error| format!("{}: {e}", p.display());
    let src = dir.join("src");
    std::fs::create_dir_all(&src).map_err(|e| io(&src, e))?;
    let name = match name {
        Some(n) => n.to_owned(),
        None => std::fs::canonicalize(dir)
            .ok()
            .and_then(|d| d.file_name().map(|n| n.to_string_lossy().into_owned()))
            .ok_or("cannot derive a crate name; pass --crate-name")?,
    };
    let lib_name = name.replace('-', "_");

    let manifest = dir.join("Cargo.toml");
    if !manifest.exists() {
        let text = format!(
            "[package]\n\
             name = \"{name}\"\n\
             version = \"0.1.0\"\n\
             edition = \"2024\"\n\
             description = \"Bindings to {headers}, loaded at run time\"\n\
             \n\
             [dependencies]\n\
             dlsym = \"{version}\"\n",
            headers = headers.join(", "),
            version = env!("CARGO_PKG_VERSION"),
        );
        std::fs::write(&manifest, text).map_err(|e| io(&manifest, e))?;
    }

    let lib_rs = src.join("lib.rs");
    if !lib_rs.exists() {
        let open = if libs.is_empty() {
            format!("{struct_name}::open(\"path/to/library\")")
        } else {
            format!("{struct_name}::open_default()")
        };
        let text = format!(
            "//! Bindings to `{headers}`, loaded at run time with [`dlsym`].\n\
             //!\n\
             //! ```no_run\n\
             //! use {lib_name}::{{Bindings, {struct_name}}};\n\
             //!\n\
             //! # fn main() -> dlsym::Result<()> {{\n\
             //! // SAFETY: the library's initialisers are trusted.\n\
             //! let lib = unsafe {{ {open} }}?;\n\
             //! # let _ = lib;\n\
             //! # Ok(())\n\
             //! # }}\n\
             //! ```\n\
             \n\
             mod bindings;\n\
             \n\
             pub use bindings::*;\n\
             pub use dlsym::{{self, Bindings}};\n",
            headers = headers.join("`, `"),
        );
        std::fs::write(&lib_rs, text).map_err(|e| io(&lib_rs, e))?;
    }

    let bindings = src.join("bindings.rs");
    std::fs::write(&bindings, code).map_err(|e| io(&bindings, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn struct_names() {
        assert_eq!(struct_name_for(Path::new("/usr/include/zlib.h")), "Zlib");
        assert_eq!(struct_name_for(Path::new("foo-bar_baz.h")), "FooBarBaz");
        assert_eq!(struct_name_for(Path::new("3d.h")), "Lib3d");
    }

    #[test]
    fn args() {
        let a = |v: &[&str]| parse_args(v.iter().map(|s| s.to_string()));
        assert!(matches!(a(&["-h"]), Ok(Command::Help)));
        assert!(a(&[]).is_err());
        assert!(a(&["x.h", "-o", "a", "--crate", "b"]).is_err());
        let Ok(Command::Run(r)) = a(&["x.h", "-l", "libx.so.1", "--", "-DFOO", "-I", "inc"]) else {
            panic!()
        };
        assert_eq!(r.libs, ["libx.so.1"]);
        assert_eq!(r.parse.clang_args, ["-DFOO", "-I", "inc"]);
        assert_eq!(r.parse.headers, [PathBuf::from("x.h")]);
    }

    #[test]
    fn args_files() {
        let words = split_args_file(
            "# zlib\n--owned gzFile=gzclose,gzopen  # handles\n\
             -l \"my lib.so\" a\"b c\"d \"q\\\"\\\\\"\n",
        )
        .unwrap();
        assert_eq!(
            words,
            [
                "--owned",
                "gzFile=gzclose,gzopen",
                "-l",
                "my lib.so",
                "ab cd",
                "q\"\\"
            ]
        );
        assert!(split_args_file("\"open").is_err());
        assert_eq!(expand_args(["@".to_string()], 0).unwrap(), ["@"]);
        assert!(expand_args(["@/nonexistent/args".to_string()], 0).is_err());
    }
}
