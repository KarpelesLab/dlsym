//! C front end: headers → [`ir`](super::ir), using libclang loaded at run
//! time through this crate's own [`library!`](crate::library).
//!
//! Declarations are *selected* when they come from one of the input headers,
//! from a header those include with `#include "..."` (or with
//! `#include <...>` when it stays in the library's own directory tree), or
//! from a file under one of [`Options::header_dirs`]. Selected functions, variables, types and
//! `#define` constants are emitted; types they reference from elsewhere
//! (system headers, other libraries) are pulled in as needed: in full when
//! used by value, as opaque types when only used behind pointers.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};

use super::clang::{self, Cursor, LibClang, Tu, Type as CType};
use super::ir::{
    Abi, Const, ConstValue, Enum, Field, Function, Header, Item, Param, Record, RecordBody,
    RecordKind, Type, Typedef, Var, Variant,
};

/// Options for [`parse`].
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// The headers to parse.
    pub headers: Vec<PathBuf>,
    /// Extra arguments for clang (`-I`, `-D`, `--target=`, ...).
    pub clang_args: Vec<String>,
    /// Also select every declaration from files under these directories.
    pub header_dirs: Vec<PathBuf>,
    /// If non-empty, only select items whose C name matches one of these
    /// glob patterns (`*` and `?`).
    pub allow: Vec<String>,
    /// Never select items whose C name matches one of these patterns.
    /// (Blocked types may still be emitted if a selected item needs them.)
    pub block: Vec<String>,
    /// Path to libclang, overriding the automatic search.
    pub libclang: Option<PathBuf>,
    /// Generate bindings even if clang reports errors.
    pub keep_going: bool,
}

/// The result of [`parse`].
#[derive(Debug, Clone)]
pub struct Parsed {
    /// The extracted declarations.
    pub header: Header,
    /// Non-fatal problems (skipped items, clang warnings).
    pub warnings: Vec<String>,
    /// The libclang version used.
    pub clang_version: String,
}

/// Errors produced while parsing headers.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// libclang could not be loaded.
    LibClang(String),
    /// libclang failed to parse the input.
    Parse(String),
    /// clang reported errors.
    Diagnostics(Vec<String>),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::LibClang(e) | Error::Parse(e) => f.write_str(e),
            Error::Diagnostics(d) => {
                writeln!(
                    f,
                    "clang reported errors (use --keep-going to ignore them):"
                )?;
                for d in d {
                    writeln!(f, "  {d}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for Error {}

const UMBRELLA: &str = "dlsym-gen-umbrella.h";
const MACRO_PREFIX: &str = "__dlsym_gen_macro_";

/// Parses the headers described by `opts`.
pub fn parse(opts: &Options) -> Result<Parsed, Error> {
    let lib = clang::load(opts.libclang.as_deref()).map_err(Error::LibClang)?;
    let clang_version = clang::version(lib);

    let mut umbrella = String::new();
    let mut inputs = HashSet::new();
    for h in &opts.headers {
        let abs =
            std::fs::canonicalize(h).map_err(|e| Error::Parse(format!("{}: {e}", h.display())))?;
        // Windows' canonical paths start with `\\?\`, which clang does
        // not understand in an `#include`.
        let text = abs.to_string_lossy();
        let text = text.strip_prefix(r"\\?\").unwrap_or(&text);
        umbrella.push_str(&format!("#include \"{text}\"\n"));
        inputs.insert(abs.to_string_lossy().into_owned());
    }
    let args = clang_args(&opts.clang_args);

    let tu = Tu::parse(
        lib,
        UMBRELLA,
        &args,
        &[(UMBRELLA.into(), umbrella.clone())],
        true,
    )
    .map_err(Error::Parse)?;
    let mut warnings = Vec::new();
    let mut errors = Vec::new();
    for d in tu.diagnostics() {
        if d.is_error {
            errors.push(d.message);
        } else {
            warnings.push(d.message);
        }
    }
    if !errors.is_empty() && !opts.keep_going {
        return Err(Error::Diagnostics(errors));
    }
    warnings.extend(errors);

    let mut ctx = Ctx::new(&tu, opts, inputs);
    ctx.warnings = warnings;
    let top = ctx.top_level();
    ctx.select_files(&top);
    ctx.name_types(&top);
    let macros = ctx.roots(&top);
    ctx.finish_records();
    let consts = evaluate_macros(lib, &args, &umbrella, &macros, &mut ctx);
    let header = ctx.take_header(consts);
    Ok(Parsed {
        header,
        warnings: ctx.warnings,
        clang_version,
    })
}

/// Builds the clang command line: the user's arguments plus defaults.
fn clang_args(user: &[String]) -> Vec<String> {
    let mut args = vec!["-xc".to_owned()];
    if cfg!(target_vendor = "apple")
        && !user.iter().any(|a| a.starts_with("-isysroot"))
        && let Ok(out) = std::process::Command::new("xcrun")
            .arg("--show-sdk-path")
            .output()
        && out.status.success()
    {
        args.push("-isysroot".into());
        args.push(String::from_utf8_lossy(&out.stdout).trim().to_owned());
    }
    args.extend(user.iter().cloned());
    args
}

/// Matches `name` against a glob with `*` and `?`.
fn glob(pattern: &str, name: &str) -> bool {
    let (p, n) = (pattern.as_bytes(), name.as_bytes());
    let (mut pi, mut ni) = (0, 0);
    let (mut star, mut mark) = (None, 0);
    while ni < n.len() {
        if pi < p.len() && (p[pi] == b'?' || p[pi] == n[ni]) {
            pi += 1;
            ni += 1;
        } else if pi < p.len() && p[pi] == b'*' {
            star = Some(pi);
            mark = ni;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ni = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == b'*' {
        pi += 1;
    }
    pi == p.len()
}

/// Well-known typedefs mapped straight to Rust primitives.
fn well_known(name: &str) -> Option<Type> {
    Some(match name {
        "int8_t" => Type::Fixed(8, true),
        "int16_t" => Type::Fixed(16, true),
        "int32_t" => Type::Fixed(32, true),
        "int64_t" => Type::Fixed(64, true),
        "uint8_t" => Type::Fixed(8, false),
        "uint16_t" => Type::Fixed(16, false),
        "uint32_t" => Type::Fixed(32, false),
        "uint64_t" => Type::Fixed(64, false),
        "size_t" | "uintptr_t" => Type::USize,
        "ssize_t" | "ptrdiff_t" | "intptr_t" => Type::ISize,
        _ => return None,
    })
}

/// Orders items by the file they come from (in order of first appearance)
/// and their offset in it.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct SortKey(usize, u32);

struct RecordReq<'tu> {
    name: String,
    kind: RecordKind,
    decl: Cursor<'tu>,
    full: bool,
    key: SortKey,
}

struct Ctx<'tu, 'o> {
    tu: &'tu Tu<'tu>,
    opts: &'o Options,
    selected: HashSet<String>,
    /// File stems of the input headers (`pcap` for `pcap.h`).
    input_stems: Vec<String>,
    header_dirs: Vec<PathBuf>,
    canon: HashMap<String, String>,
    file_rank: HashMap<String, usize>,
    /// Rust names for tag declarations, by USR.
    tag_names: HashMap<String, String>,
    /// Typedef names, mapped to the USR of the tag they alias (if any).
    typedef_targets: HashMap<String, Option<String>>,
    records: HashMap<String, RecordReq<'tu>>,
    record_work: Vec<String>,
    bodies: HashMap<String, RecordBody>,
    enums: HashMap<String, Option<String>>,
    /// Emitted typedefs, and whether their target is needed by value.
    typedefs: HashMap<String, bool>,
    types: Vec<(SortKey, Item)>,
    funcs: Vec<(SortKey, Item)>,
    func_names: HashSet<String>,
    value_names: HashSet<String>,
    warnings: Vec<String>,
    anon_count: usize,
}

impl<'tu, 'o> Ctx<'tu, 'o> {
    fn new(tu: &'tu Tu<'tu>, opts: &'o Options, inputs: HashSet<String>) -> Self {
        Ctx {
            tu,
            opts,
            selected: inputs,
            input_stems: opts
                .headers
                .iter()
                .filter_map(|h| h.file_stem()?.to_str().map(str::to_owned))
                .collect(),
            header_dirs: opts
                .header_dirs
                .iter()
                .map(|d| std::fs::canonicalize(d).unwrap_or_else(|_| d.clone()))
                .collect(),
            canon: HashMap::new(),
            file_rank: HashMap::new(),
            tag_names: HashMap::new(),
            typedef_targets: HashMap::new(),
            records: HashMap::new(),
            record_work: Vec::new(),
            bodies: HashMap::new(),
            enums: HashMap::new(),
            typedefs: HashMap::new(),
            types: Vec::new(),
            funcs: Vec::new(),
            func_names: HashSet::new(),
            value_names: HashSet::new(),
            warnings: Vec::new(),
            anon_count: 0,
        }
    }

    fn warn(&mut self, msg: String) {
        self.warnings.push(msg);
    }

    /// Top-level cursors, looking through `extern "C"` blocks.
    fn top_level(&self) -> Vec<Cursor<'tu>> {
        let mut out = Vec::new();
        for c in self.tu.cursor().children() {
            if c.kind() == clang::LINKAGE_SPEC {
                out.extend(c.children());
            } else {
                out.push(c);
            }
        }
        out
    }

    fn canonical_file(&mut self, file: &str) -> String {
        if let Some(c) = self.canon.get(file) {
            return c.clone();
        }
        let c = std::fs::canonicalize(file)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| file.to_owned());
        self.canon.insert(file.to_owned(), c.clone());
        c
    }

    fn key(&mut self, c: Cursor<'tu>) -> SortKey {
        let loc = c.location();
        let file = self.canonical_file(&loc.file);
        let n = self.file_rank.len();
        let rank = *self.file_rank.entry(file).or_insert(n);
        SortKey(rank, loc.offset)
    }

    fn is_selected(&mut self, c: Cursor<'tu>) -> bool {
        let file = c.location().file;
        if file.is_empty() {
            return false;
        }
        let file = self.canonical_file(&file);
        self.selected.contains(&file)
            || self
                .header_dirs
                .iter()
                .any(|d| Path::new(&file).starts_with(d))
    }

    fn name_allowed(&self, name: &str) -> bool {
        (self.opts.allow.is_empty() || self.opts.allow.iter().any(|p| glob(p, name)))
            && !self.opts.block.iter().any(|p| glob(p, name))
    }

    /// Adds files included with `#include "..."` from selected files.
    fn select_files(&mut self, top: &[Cursor<'tu>]) {
        for &c in top {
            if c.kind() != clang::INCLUSION_DIRECTIVE {
                continue;
            }
            let from = self.canonical_file(&c.location().file);
            if !self.selected.contains(&from) {
                continue;
            }
            let Some(file) = c.included_file() else {
                continue;
            };
            let file = self.canonical_file(&file);
            let quoted = self
                .tu
                .tokens(c)
                .iter()
                .any(|(literal, s)| *literal && s.starts_with('"'));
            if quoted || self.same_library(Path::new(&from), Path::new(&file)) {
                self.selected.insert(file);
            }
        }
    }

    /// Whether an `#include <...>` from `from` stays within the same
    /// library: the included file is in `from`'s directory tree, and that
    /// directory is not a system include root (then only a subdirectory
    /// named after an input header counts, e.g. `pcap.h` -> `<pcap/...>`).
    fn same_library(&self, from: &Path, file: &Path) -> bool {
        let Some(dir) = from.parent() else {
            return false;
        };
        if !file.starts_with(dir) {
            return false;
        }
        let is_root = ["stdio.h", "stddef.h", "stdlib.h"]
            .iter()
            .any(|h| dir.join(h).exists());
        !is_root
            || self.input_stems.iter().any(|stem| {
                file.strip_prefix(dir)
                    .ok()
                    .and_then(|rest| rest.components().next())
                    .is_some_and(|first| first.as_os_str() == stem.as_str())
            })
    }

    /// Assigns names to anonymous tags declared through a typedef, and
    /// records which tag each typedef aliases (to detect name clashes).
    fn name_types(&mut self, top: &[Cursor<'tu>]) {
        for c in top {
            if c.kind() != clang::TYPEDEF_DECL {
                continue;
            }
            let name = c.spelling();
            let target = desugar(c.typedef_underlying());
            let usr = matches!(target.kind(), clang::TYPE_RECORD | clang::TYPE_ENUM)
                .then(|| target.declaration().canonical().usr());
            if let Some(usr) = &usr
                && target.declaration().is_anonymous()
            {
                self.tag_names
                    .entry(usr.clone())
                    .or_insert_with(|| name.clone());
            }
            self.typedef_targets.entry(name).or_insert(usr);
        }
    }

    fn tag_name(&mut self, decl: Cursor<'tu>, hint: Option<&str>) -> Option<String> {
        let decl = decl.canonical();
        let usr = decl.usr();
        if let Some(n) = self.tag_names.get(&usr) {
            return Some(n.clone());
        }
        let name = if decl.is_anonymous() {
            hint?.to_owned()
        } else {
            let name = decl.spelling();
            // `typedef int foo; struct foo {..};` would clash in Rust.
            match self.typedef_targets.get(&name) {
                Some(target) if target.as_deref() != Some(usr.as_str()) => {
                    let prefix = match decl.kind() {
                        clang::UNION_DECL => "union",
                        clang::ENUM_DECL => "enum",
                        _ => "struct",
                    };
                    format!("{prefix}_{name}")
                }
                _ => name,
            }
        };
        self.tag_names.insert(usr, name.clone());
        Some(name)
    }

    /// Visits selected declarations. Returns the `#define`s to evaluate.
    fn roots(&mut self, top: &[Cursor<'tu>]) -> Vec<(String, Cursor<'tu>)> {
        let mut macros = Vec::new();
        for &c in top {
            let kind = c.kind();
            if kind == clang::INCLUSION_DIRECTIVE || !self.is_selected(c) {
                continue;
            }
            let name = c.spelling();
            match kind {
                clang::FUNCTION_DECL if self.name_allowed(&name) => self.add_function(c),
                clang::VAR_DECL if self.name_allowed(&name) => self.add_var(c),
                clang::STRUCT_DECL | clang::UNION_DECL
                    if !c.is_anonymous() && self.name_allowed(&name) =>
                {
                    let full = c.definition().is_some();
                    self.require_record(c, full);
                }
                clang::ENUM_DECL
                    if (c.is_anonymous() || self.name_allowed(&name))
                        && c.definition().is_some() =>
                {
                    self.require_enum(c);
                }
                clang::TYPEDEF_DECL
                    if self.name_allowed(&name)
                        && !is_function_type(desugar(c.typedef_underlying())) =>
                {
                    self.require_typedef(c, true);
                }
                clang::MACRO_DEFINITION
                    if !c.is_function_like_macro()
                        && !c.is_builtin_macro()
                        && !name.starts_with("__")
                        && self.name_allowed(&name) =>
                {
                    macros.push((name, c));
                }
                _ => {}
            }
        }
        macros
    }

    fn add_function(&mut self, c: Cursor<'tu>) {
        let name = c.spelling();
        if c.linkage() != clang::LINKAGE_EXTERNAL
            || c.is_definition()
            || self.func_names.contains(&name)
        {
            return;
        }
        let ty = c.cursor_type();
        let params: Vec<Param> = c
            .arguments()
            .into_iter()
            .map(|a| Param {
                name: Some(a.spelling()).filter(|n| !n.is_empty()),
                ty: self.convert_param(a.cursor_type()),
            })
            .collect();
        let ret = self.convert(c.result_type(), true);
        let f = Function {
            name: name.clone(),
            params,
            ret,
            variadic: ty.kind() == clang::TYPE_FUNCTION_PROTO && ty.is_variadic(),
            abi: abi(ty.calling_convention()),
            doc: c.comment(),
        };
        let bad = f
            .params
            .iter()
            .map(|p| &p.ty)
            .chain([&f.ret])
            .find_map(Type::unsupported)
            .map(str::to_owned);
        if let Some(what) = bad {
            self.warn(format!(
                "skipped function `{name}`: unsupported type `{what}`"
            ));
            return;
        }
        self.func_names.insert(name);
        let key = self.key(c);
        self.funcs.push((key, Item::Function(f)));
    }

    fn add_var(&mut self, c: Cursor<'tu>) {
        let name = c.spelling();
        if c.linkage() != clang::LINKAGE_EXTERNAL
            || c.storage_class() == clang::STORAGE_STATIC
            || self.func_names.contains(&name)
        {
            return;
        }
        let t = c.cursor_type();
        // An `extern T name[];` symbol is the address of its first element.
        let (ty, is_const) = if desugar(t).kind() == clang::TYPE_INCOMPLETE_ARRAY {
            let elem = desugar(t).element();
            (self.convert(elem, true), elem.is_const())
        } else {
            (self.convert(t, true), t.is_const())
        };
        if let Some(what) = ty.unsupported() {
            let what = what.to_owned();
            self.warn(format!(
                "skipped variable `{name}`: unsupported type `{what}`"
            ));
            return;
        }
        self.func_names.insert(name.clone());
        let key = self.key(c);
        let doc = c.comment();
        self.funcs.push((
            key,
            Item::Var(Var {
                name,
                ty,
                is_const,
                doc,
            }),
        ));
    }

    fn convert_param(&mut self, t: CType<'tu>) -> Type {
        let d = desugar(t);
        match d.kind() {
            clang::TYPE_CONSTANT_ARRAY | clang::TYPE_INCOMPLETE_ARRAY => {
                let elem = d.element();
                Type::Pointer {
                    pointee: Box::new(self.convert(elem, false)),
                    is_const: elem.is_const(),
                }
            }
            clang::TYPE_FUNCTION_PROTO | clang::TYPE_FUNCTION_NO_PROTO => self.fn_ptr(d),
            _ => self.convert(t, true),
        }
    }

    fn convert(&mut self, t: CType<'tu>, by_value: bool) -> Type {
        match t.kind() {
            clang::TYPE_ELABORATED => self.convert(t.named(), by_value),
            clang::TYPE_ATTRIBUTED => match t.modified() {
                Some(m) => self.convert(m, by_value),
                None => self.convert(t.canonical(), by_value),
            },
            clang::TYPE_TYPEDEF => self.convert_typedef(t, by_value),
            clang::TYPE_RECORD => self.require_record(t.declaration(), by_value),
            clang::TYPE_ENUM => self.require_enum(t.declaration()),
            clang::TYPE_POINTER => {
                let pointee = t.pointee();
                if is_function_type(desugar(pointee)) {
                    return self.fn_ptr(desugar(pointee));
                }
                Type::Pointer {
                    pointee: Box::new(self.convert(pointee, false)),
                    is_const: pointee.is_const(),
                }
            }
            clang::TYPE_CONSTANT_ARRAY => Type::Array {
                elem: Box::new(self.convert(t.element(), by_value)),
                len: t.array_len().unwrap_or(0),
            },
            clang::TYPE_INCOMPLETE_ARRAY => Type::Array {
                elem: Box::new(self.convert(t.element(), by_value)),
                len: 0,
            },
            clang::TYPE_UNEXPOSED | clang::TYPE_INVALID => {
                let c = t.canonical();
                if c.kind() != clang::TYPE_UNEXPOSED && c.kind() != clang::TYPE_INVALID {
                    self.convert(c, by_value)
                } else {
                    Type::Unsupported(t.spelling())
                }
            }
            kind => builtin(kind, t),
        }
    }

    fn fn_ptr(&mut self, ft: CType<'tu>) -> Type {
        let params = ft
            .args()
            .into_iter()
            .map(|a| self.convert_param(a))
            .collect();
        Type::FnPtr {
            params,
            ret: Box::new(self.convert(ft.result(), true)),
            variadic: ft.kind() == clang::TYPE_FUNCTION_PROTO && ft.is_variadic(),
            abi: abi(ft.calling_convention()),
        }
    }

    fn convert_typedef(&mut self, t: CType<'tu>, by_value: bool) -> Type {
        let decl = t.declaration();
        let name = decl.spelling();
        if let Some(prim) = well_known(&name) {
            return prim;
        }
        let under = desugar(t);
        if is_function_type(under) {
            return Type::Unsupported(format!("function type `{name}`"));
        }
        // Keep the names of the library's own typedefs, and of typedefs for
        // records and enums (`FILE`); inline other foreign ones (`off_t`).
        if self.is_selected(decl) || matches!(under.kind(), clang::TYPE_RECORD | clang::TYPE_ENUM) {
            self.require_typedef(decl, by_value)
        } else {
            self.convert(decl.typedef_underlying(), by_value)
        }
    }

    fn require_typedef(&mut self, decl: Cursor<'tu>, by_value: bool) -> Type {
        let name = decl.spelling();
        if let Some(prim) = well_known(&name) {
            return prim;
        }
        match self.typedefs.get(&name) {
            Some(&full) if full || !by_value => return Type::Named(name),
            Some(_) => {
                // A pointer-only use became a by-value one: the target must
                // now be complete.
                self.typedefs.insert(name.clone(), true);
                self.convert(decl.typedef_underlying(), true);
                return Type::Named(name);
            }
            None => {}
        }
        self.typedefs.insert(name.clone(), by_value);
        let ty = self.convert(decl.typedef_underlying(), by_value);
        // `typedef struct foo foo;` and `typedef struct {..} foo;` need no
        // alias: the record already has this name.
        if ty != Type::Named(name.clone()) {
            if let Some(what) = ty.unsupported() {
                let what = what.to_owned();
                self.warn(format!(
                    "skipped typedef `{name}`: unsupported type `{what}`"
                ));
                return Type::Unsupported(what);
            }
            let key = self.key(decl);
            let doc = decl.comment();
            self.types.push((
                key,
                Item::Typedef(Typedef {
                    name: name.clone(),
                    ty,
                    doc,
                }),
            ));
        }
        Type::Named(name)
    }

    fn require_record(&mut self, decl: Cursor<'tu>, by_value: bool) -> Type {
        let decl = decl.canonical();
        let usr = decl.usr();
        let name = match self.tag_name(decl, None) {
            Some(n) => n,
            None => {
                self.anon_count += 1;
                let n = format!("anon{}", self.anon_count);
                self.tag_names.insert(usr.clone(), n.clone());
                n
            }
        };
        if let Some(req) = self.records.get_mut(&usr) {
            if by_value && !req.full {
                req.full = true;
                self.record_work.push(usr);
            }
            return Type::Named(name);
        }
        let kind = if decl.kind() == clang::UNION_DECL {
            RecordKind::Union
        } else {
            RecordKind::Struct
        };
        let key = self.key(decl.definition().unwrap_or(decl));
        self.records.insert(
            usr.clone(),
            RecordReq {
                name: name.clone(),
                kind,
                decl,
                full: by_value,
                key,
            },
        );
        if by_value {
            self.record_work.push(usr);
        }
        Type::Named(name)
    }

    fn require_enum(&mut self, decl: Cursor<'tu>) -> Type {
        let decl = decl.definition().unwrap_or(decl);
        let usr = decl.canonical().usr();
        let repr = integer_type(decl.enum_integer_type());
        if let Some(name) = self.enums.get(&usr) {
            return name.clone().map_or(repr, Type::Named);
        }
        let name = self.tag_name(decl, None);
        self.enums.insert(usr, name.clone());
        let unsigned = matches!(
            repr,
            Type::UChar
                | Type::UShort
                | Type::UInt
                | Type::ULong
                | Type::ULongLong
                | Type::Fixed(_, false)
        );
        let variants: Vec<Variant> = decl
            .children()
            .into_iter()
            .filter(|v| v.kind() == clang::ENUM_CONSTANT_DECL)
            .map(|v| Variant {
                name: v.spelling(),
                value: v.enum_value(unsigned),
                doc: v.comment(),
            })
            .collect();
        self.value_names
            .extend(variants.iter().map(|v| v.name.clone()));
        self.value_names.extend(name.clone());
        let key = self.key(decl);
        self.types.push((
            key,
            Item::Enum(Enum {
                name: name.clone(),
                repr: repr.clone(),
                variants,
                doc: decl.comment(),
            }),
        ));
        name.map_or(repr, Type::Named)
    }

    /// Converts the fields of every record needed by value, until no new
    /// requirement appears.
    fn finish_records(&mut self) {
        let mut done = HashSet::new();
        while let Some(usr) = self.record_work.pop() {
            if !done.insert(usr.clone()) {
                continue;
            }
            let (decl, name) = {
                let r = &self.records[&usr];
                (r.decl, r.name.clone())
            };
            let Some(def) = decl.definition() else {
                continue;
            };
            let body = self.record_body(def, &name);
            if let Some(r) = self.records.get_mut(&usr) {
                r.decl = def;
            }
            if let Some(body) = body {
                self.bodies.insert(usr, body);
            }
        }
    }

    fn record_body(&mut self, def: Cursor<'tu>, parent: &str) -> Option<RecordBody> {
        let ty = def.cursor_type();
        let (size, align) = (ty.size()?, ty.align()?);
        let mut fields = Vec::new();
        let mut anon = 0;
        for c in def.children() {
            match c.kind() {
                clang::FIELD_DECL => {
                    let raw = c.spelling();
                    let fty = c.cursor_type();
                    let name = if raw.is_empty() {
                        anon += 1;
                        format!("__anon{anon}")
                    } else {
                        raw
                    };
                    self.name_anonymous(desugar(fty), &format!("{parent}_{name}"));
                    let ty = self.convert(fty, true);
                    fields.push(Field {
                        name,
                        ty,
                        offset_bits: c.field_offset_bits().unwrap_or(0),
                        bit_width: c.bit_width(),
                        layout: (fty.size().unwrap_or(0), fty.align().unwrap_or(1)),
                        doc: c.comment(),
                    });
                }
                clang::STRUCT_DECL | clang::UNION_DECL if c.is_anonymous_member() => {
                    anon += 1;
                    let name = format!("__anon{anon}");
                    let rty = c.cursor_type();
                    self.name_anonymous(rty, &format!("{parent}_{name}"));
                    let offset = first_field_name(c)
                        .and_then(|n| ty.offset_of(&n))
                        .unwrap_or(0);
                    let fty = self.convert(rty, true);
                    fields.push(Field {
                        name,
                        ty: fty,
                        offset_bits: offset,
                        bit_width: None,
                        layout: (rty.size().unwrap_or(0), rty.align().unwrap_or(1)),
                        doc: None,
                    });
                }
                _ => {}
            }
        }
        if let Some(bad) = fields.iter().find_map(|f| f.ty.unsupported()) {
            let bad = bad.to_owned();
            self.warn(format!(
                "`{parent}` is opaque: it has a field of unsupported type `{bad}`"
            ));
            return None;
        }
        Some(RecordBody {
            size,
            align,
            fields,
        })
    }

    fn name_anonymous(&mut self, t: CType<'tu>, hint: &str) {
        let t = match t.kind() {
            clang::TYPE_CONSTANT_ARRAY | clang::TYPE_INCOMPLETE_ARRAY => desugar(t.element()),
            _ => t,
        };
        if matches!(t.kind(), clang::TYPE_RECORD | clang::TYPE_ENUM)
            && t.declaration().is_anonymous()
        {
            self.tag_name(t.declaration(), Some(hint));
        }
    }

    fn take_header(&mut self, consts: Vec<Item>) -> Header {
        let mut types = std::mem::take(&mut self.types);
        for (usr, r) in self.records.drain() {
            let body = if r.full {
                self.bodies.remove(&usr)
            } else {
                None
            };
            types.push((
                r.key,
                Item::Record(Record {
                    name: r.name,
                    kind: r.kind,
                    body,
                    doc: r.decl.comment(),
                }),
            ));
        }
        types.sort_by_key(|(k, _)| *k);
        let mut funcs = std::mem::take(&mut self.funcs);
        funcs.sort_by_key(|(k, _)| *k);
        Header {
            items: types
                .into_iter()
                .map(|(_, i)| i)
                .chain(consts)
                .chain(funcs.into_iter().map(|(_, i)| i))
                .collect(),
        }
    }
}

fn first_field_name(record: Cursor<'_>) -> Option<String> {
    for c in record.children() {
        match c.kind() {
            clang::FIELD_DECL if !c.spelling().is_empty() => return Some(c.spelling()),
            clang::STRUCT_DECL | clang::UNION_DECL if c.is_anonymous_member() => {
                if let Some(n) = first_field_name(c) {
                    return Some(n);
                }
            }
            _ => {}
        }
    }
    None
}

/// Converts an integer type, looking through typedefs.
fn integer_type(t: CType<'_>) -> Type {
    if t.kind() == clang::TYPE_TYPEDEF
        && let Some(prim) = well_known(&t.declaration().spelling())
    {
        return prim;
    }
    let c = t.canonical();
    builtin(c.kind(), c)
}

/// Strips typedefs, elaboration and attributes.
fn desugar(mut t: CType<'_>) -> CType<'_> {
    loop {
        t = match t.kind() {
            clang::TYPE_ELABORATED => t.named(),
            clang::TYPE_TYPEDEF => t.declaration().typedef_underlying(),
            clang::TYPE_ATTRIBUTED => t.modified().unwrap_or_else(|| t.canonical()),
            _ => return t,
        };
    }
}

fn is_function_type(t: CType<'_>) -> bool {
    matches!(
        t.kind(),
        clang::TYPE_FUNCTION_PROTO | clang::TYPE_FUNCTION_NO_PROTO
    )
}

fn abi(cc: std::ffi::c_int) -> Abi {
    match cc {
        clang::CALLCONV_X86_STDCALL => Abi::Stdcall,
        clang::CALLCONV_X86_FASTCALL => Abi::Fastcall,
        clang::CALLCONV_X86_THISCALL => Abi::Thiscall,
        clang::CALLCONV_X86_VECTORCALL => Abi::Vectorcall,
        _ => Abi::C,
    }
}

fn builtin(kind: std::ffi::c_int, t: CType<'_>) -> Type {
    match kind {
        clang::TYPE_VOID => Type::Void,
        clang::TYPE_BOOL => Type::Bool,
        clang::TYPE_CHAR_S | clang::TYPE_CHAR_U => Type::Char,
        clang::TYPE_SCHAR => Type::SChar,
        clang::TYPE_UCHAR => Type::UChar,
        clang::TYPE_SHORT => Type::Short,
        clang::TYPE_USHORT => Type::UShort,
        clang::TYPE_INT => Type::Int,
        clang::TYPE_UINT => Type::UInt,
        clang::TYPE_LONG => Type::Long,
        clang::TYPE_ULONG => Type::ULong,
        clang::TYPE_LONGLONG => Type::LongLong,
        clang::TYPE_ULONGLONG => Type::ULongLong,
        clang::TYPE_INT128 => Type::Fixed(128, true),
        clang::TYPE_UINT128 => Type::Fixed(128, false),
        clang::TYPE_FLOAT => Type::Float,
        clang::TYPE_DOUBLE => Type::Double,
        clang::TYPE_CHAR16 => Type::Fixed(16, false),
        clang::TYPE_CHAR32 => Type::Fixed(32, false),
        clang::TYPE_WCHAR => match t.size() {
            Some(2) => Type::Fixed(16, false),
            _ => Type::Fixed(32, true),
        },
        clang::TYPE_NULLPTR => Type::ptr(Type::Void),
        _ => Type::Unsupported(t.spelling()),
    }
}

/// Evaluates object-like macros.
///
/// String macros are decoded from their tokens. Others are evaluated by
/// appending `static const __auto_type PROBE = (NAME);` after the headers and
/// asking clang for each initializer's value and type; macros that are not
/// constant expressions simply fail to evaluate and are skipped.
fn evaluate_macros(
    lib: &LibClang,
    args: &[String],
    umbrella: &str,
    macros: &[(String, Cursor<'_>)],
    ctx: &mut Ctx<'_, '_>,
) -> Vec<Item> {
    let mut out = Vec::new();
    let mut probes = String::from(umbrella);
    let mut numeric = Vec::new();
    for (name, cursor) in macros {
        if ctx.value_names.contains(name) {
            continue;
        }
        let tokens = ctx.tu.tokens(*cursor);
        let body = tokens.get(1..).unwrap_or_default();
        if body.is_empty() {
            continue;
        }
        if body.iter().all(|(lit, s)| *lit && s.starts_with('"')) {
            let bytes: Option<Vec<u8>> = body
                .iter()
                .map(|(_, s)| unescape_c_string(s))
                .collect::<Option<Vec<_>>>()
                .map(|parts| parts.concat());
            if let Some(bytes) = bytes.filter(|b| !b.contains(&0)) {
                ctx.value_names.insert(name.clone());
                out.push(Item::Const(Const {
                    name: name.clone(),
                    value: ConstValue::Str(bytes),
                    doc: None,
                }));
            }
            continue;
        }
        probes.push_str(&format!(
            "\nstatic const __auto_type {MACRO_PREFIX}{} = ({name});",
            numeric.len()
        ));
        numeric.push(name.clone());
    }
    if numeric.is_empty() {
        return out;
    }
    let Ok(tu) = Tu::parse(lib, UMBRELLA, args, &[(UMBRELLA.into(), probes)], false) else {
        return out;
    };
    for c in tu.cursor().children() {
        let spelling = c.spelling();
        let Some(i) = spelling
            .strip_prefix(MACRO_PREFIX)
            .and_then(|i| i.parse::<usize>().ok())
        else {
            continue;
        };
        let name = &numeric[i];
        let ty = integer_type(c.cursor_type());
        let value = match c.evaluate() {
            Some(clang::Evaluated::Int(v)) if is_integer(&ty) => ConstValue::Int(v, ty),
            Some(clang::Evaluated::Float(v)) if matches!(ty, Type::Float | Type::Double) => {
                ConstValue::Float(v, ty)
            }
            _ => continue,
        };
        ctx.value_names.insert(name.clone());
        out.push(Item::Const(Const {
            name: name.clone(),
            value,
            doc: None,
        }));
    }
    out
}

fn is_integer(t: &Type) -> bool {
    matches!(
        t,
        Type::Bool
            | Type::Char
            | Type::SChar
            | Type::UChar
            | Type::Short
            | Type::UShort
            | Type::Int
            | Type::UInt
            | Type::Long
            | Type::ULong
            | Type::LongLong
            | Type::ULongLong
            | Type::Fixed(..)
            | Type::USize
            | Type::ISize
    )
}

/// Decodes a C string literal token (with quotes) into bytes.
fn unescape_c_string(token: &str) -> Option<Vec<u8>> {
    let inner = token.strip_prefix('"')?.strip_suffix('"')?.as_bytes();
    let mut out = Vec::with_capacity(inner.len());
    let mut i = 0;
    while i < inner.len() {
        let b = inner[i];
        i += 1;
        if b != b'\\' {
            out.push(b);
            continue;
        }
        let e = *inner.get(i)?;
        i += 1;
        out.push(match e {
            b'n' => b'\n',
            b't' => b'\t',
            b'r' => b'\r',
            b'a' => 7,
            b'b' => 8,
            b'f' => 12,
            b'v' => 11,
            b'e' => 27,
            b'\\' | b'\'' | b'"' | b'?' => e,
            b'x' => {
                let start = i;
                while i < inner.len() && inner[i].is_ascii_hexdigit() {
                    i += 1;
                }
                u8::from_str_radix(std::str::from_utf8(&inner[start..i]).ok()?, 16).ok()?
            }
            b'0'..=b'7' => {
                let start = i - 1;
                while i < inner.len() && i - start < 3 && (b'0'..=b'7').contains(&inner[i]) {
                    i += 1;
                }
                u8::from_str_radix(std::str::from_utf8(&inner[start..i]).ok()?, 8).ok()?
            }
            _ => return None,
        });
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globs() {
        assert!(glob("z*", "zlibVersion"));
        assert!(glob("*Init?", "deflateInit2"));
        assert!(!glob("z*", "deflate"));
        assert!(glob("*", ""));
        assert!(glob("a*b*c", "aXXbYYc"));
        assert!(!glob("a*b*c", "aXXbYY"));
    }

    #[test]
    fn c_strings() {
        assert_eq!(unescape_c_string(r#""1.3.2""#).unwrap(), b"1.3.2");
        assert_eq!(unescape_c_string(r#""a\n\x41\101\"""#).unwrap(), b"a\nAA\"");
        assert_eq!(unescape_c_string("\"\\q\""), None);
    }
}
