//! `dlsym-gen`: generate safe `dlsym` bindings from a C header.

use std::path::PathBuf;
use std::process::ExitCode;

use dlsym_gen::{emit, parse};

const USAGE: &str = "\
usage: dlsym-gen [options] <header.h> [-- <clang args>...]

options:
    -s, --struct <Name>   name of the generated bindings struct (default: Bindings)
    -o, --output <file>   write to <file> instead of stdout
    -h, --help            show this help
";

struct Args {
    header: PathBuf,
    struct_name: String,
    output: Option<PathBuf>,
    clang_args: Vec<String>,
}

fn parse_args() -> Result<Option<Args>, String> {
    let mut args = std::env::args().skip(1);
    let mut header = None;
    let mut struct_name = "Bindings".to_owned();
    let mut output = None;
    let mut clang_args = Vec::new();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(None),
            "-s" | "--struct" => struct_name = args.next().ok_or("--struct needs a value")?,
            "-o" | "--output" => output = Some(args.next().ok_or("--output needs a value")?.into()),
            "--" => clang_args.extend(args.by_ref()),
            a if a.starts_with('-') => return Err(format!("unknown option {a}")),
            a if header.is_none() => header = Some(PathBuf::from(a)),
            a => return Err(format!("unexpected argument {a}")),
        }
    }

    let header = header.ok_or("missing header path")?;
    Ok(Some(Args {
        header,
        struct_name,
        output,
        clang_args,
    }))
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(Some(args)) => args,
        Ok(None) => {
            print!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprint!("dlsym-gen: {e}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };

    let header = match parse::parse_header(&args.header, &args.clang_args) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("dlsym-gen: {}: {e}", args.header.display());
            return ExitCode::FAILURE;
        }
    };

    let code = emit::emit(
        &header,
        &emit::Options {
            struct_name: args.struct_name,
        },
    );

    let result = match &args.output {
        Some(path) => std::fs::write(path, code),
        None => {
            print!("{code}");
            Ok(())
        }
    };
    if let Err(e) = result {
        eprintln!("dlsym-gen: failed to write output: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
