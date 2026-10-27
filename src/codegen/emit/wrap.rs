//! Typed wrappers (`--wrap`): `&CStr` for C strings, slices for
//! `(pointer, length)` pairs, out-parameters returned, and error codes
//! turned into `Result`.

use std::fmt::Write;

use super::{Emitter, Options, glob, ident, rust_type};
use crate::codegen::ir::{Function, Header, Item, Type};

/// Annotations for the functions matching a glob, parsed from
/// `GLOB:ANNOTATION,...`:
///
/// | Annotation      | Effect                                                          |
/// |-----------------|-----------------------------------------------------------------|
/// | `str`           | every `const char *` parameter becomes `&CStr`                  |
/// | `str=NAME`      | only parameter `NAME` becomes `&CStr`                           |
/// | `slice=PTR/LEN` | `PTR` becomes `&[T]` (`&mut [T]` if not `const`; `u8` for `void` and `char`), and `LEN` is its length |
/// | `out=NAME`      | `NAME` (a `T *`) is removed and its value returned              |
/// | `ret=nonzero`   | a non-zero return is `Err(code)`; `Ok` holds the out-parameters |
/// | `ret=negative`  | a negative return is `Err(code)`; `Ok` holds the return value and out-parameters |
/// | `ret=errno`     | a negative return is `Err(std::io::Error::last_os_error())`     |
/// | `ret=static-str`| a returned `const char *` is `Option<&CStr>`, borrowed from the bindings (for strings that live as long as the library, like version strings) |
/// | `safe`          | the wrapper is a safe `fn`: the author asserts the function is sound for any values of the remaining parameters |
///
/// Library functions get the wrapper as a `{name}_rs` method next to the
/// raw one; methods of [`Owned`](super::Owned) handles and C++ objects, and
/// their constructors, use the annotations directly.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Wrap {
    /// Glob (`*`, `?`) of the functions the annotations apply to.
    pub pattern: String,
    /// `str`: every `const char *` parameter.
    pub all_strings: bool,
    /// `str=NAME` parameters.
    pub strings: Vec<String>,
    /// `slice=PTR/LEN` pairs.
    pub slices: Vec<(String, String)>,
    /// `out=NAME` parameters.
    pub outs: Vec<String>,
    /// How the return value signals errors.
    pub ret: Option<Ret>,
    /// Whether the wrapper is safe to call.
    pub safe: bool,
}

/// How a wrapped function's return value is interpreted; see [`Wrap`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ret {
    /// Zero is success; anything else is the error code.
    NonZero,
    /// Negative values are error codes.
    Negative,
    /// Negative values mean failure, with the reason in `errno`.
    Errno,
    /// A C string that lives as long as the library.
    StaticStr,
}

impl Wrap {
    /// Parses `GLOB:ANNOTATION,...` (see [`Wrap`]).
    pub fn parse(spec: &str) -> Result<Self, String> {
        let (pattern, annotations) = spec
            .split_once(':')
            .ok_or_else(|| format!("expected GLOB:ANNOTATION,..., got {spec:?}"))?;
        let mut w = Wrap {
            pattern: pattern.trim().to_owned(),
            ..Wrap::default()
        };
        for a in annotations
            .split(',')
            .map(str::trim)
            .filter(|a| !a.is_empty())
        {
            let (key, value) = match a.split_once('=') {
                Some((k, v)) => (k.trim(), Some(v.trim())),
                None => (a, None),
            };
            match (key, value) {
                ("str", None) => w.all_strings = true,
                ("str", Some(name)) => w.strings.push(name.to_owned()),
                ("slice", Some(pair)) => {
                    let (ptr, len) = pair
                        .split_once('/')
                        .ok_or_else(|| format!("expected slice=PTR/LEN, got {a:?}"))?;
                    w.slices
                        .push((ptr.trim().to_owned(), len.trim().to_owned()));
                }
                ("out", Some(name)) => w.outs.push(name.to_owned()),
                ("ret", Some(kind)) => {
                    w.ret = Some(match kind {
                        "nonzero" => Ret::NonZero,
                        "negative" => Ret::Negative,
                        "errno" => Ret::Errno,
                        "static-str" => Ret::StaticStr,
                        _ => return Err(format!("unknown ret= kind {kind:?} in {spec:?}")),
                    })
                }
                ("safe", None) => w.safe = true,
                _ => return Err(format!("unknown annotation {a:?} in {spec:?}")),
            }
        }
        Ok(w)
    }

    /// Merges the annotations of every spec matching `name`, or `None` if
    /// none does.
    pub(super) fn find(wraps: &[Wrap], name: &str) -> Option<Wrap> {
        let mut found: Option<Wrap> = None;
        for w in wraps.iter().filter(|w| glob(&w.pattern, name)) {
            let m = found.get_or_insert_with(Wrap::default);
            m.all_strings |= w.all_strings;
            m.strings.extend(w.strings.iter().cloned());
            m.slices.extend(w.slices.iter().cloned());
            m.outs.extend(w.outs.iter().cloned());
            m.ret = w.ret.or(m.ret);
            m.safe |= w.safe;
        }
        found
    }
}

/// A wrapper's signature and body pieces.
pub(super) struct Plan {
    /// `, name: Type` for each Rust parameter.
    pub params: String,
    /// Statements before the call.
    pub setup: String,
    /// The C arguments after the skipped ones, as Rust expressions.
    pub args: Vec<String>,
    /// The return type (without ` -> `), and the expression producing it
    /// from `ret` (the raw return value) and the out-parameters.
    pub ret_ty: String,
    pub ret_expr: String,
    /// Whether the call's result is used.
    pub uses_ret: bool,
}

impl Emitter<'_> {
    /// Plans a wrapper for `f`, whose first `skip` parameters are supplied
    /// by the caller (e.g. a handle). `None` if an annotation does not fit
    /// (with the reason).
    pub(super) fn plan(&self, f: &Function, skip: usize, w: &Wrap) -> Result<Plan, String> {
        let params = &f.params[skip..];
        let names: Vec<String> = params
            .iter()
            .enumerate()
            .map(|(i, p)| p.name.clone().unwrap_or_else(|| format!("arg{}", i + skip)))
            .collect();
        let find = |n: &str| {
            names
                .iter()
                .position(|m| m == n)
                .ok_or_else(|| format!("no parameter `{n}`"))
        };
        // How each C parameter is supplied.
        enum Arg {
            Plain,
            Str,
            Slice { mutable: bool, elem: String },
            Len(usize),
            Out(String),
        }
        let mut how: Vec<Arg> = params.iter().map(|_| Arg::Plain).collect();
        for (i, p) in params.iter().enumerate() {
            let wanted = w.strings.contains(&names[i]);
            if (w.all_strings || wanted) && self.is_c_string(&p.ty) {
                how[i] = Arg::Str;
            } else if wanted {
                return Err(format!("`{}` is not a `const char *`", names[i]));
            }
        }
        for (ptr, len) in &w.slices {
            let (p, l) = (find(ptr)?, find(len)?);
            let Type::Pointer { pointee, is_const } = self.resolve(&params[p].ty) else {
                return Err(format!("`{ptr}` is not a pointer"));
            };
            if !is_integer(self.resolve(&params[l].ty)) {
                return Err(format!("`{len}` is not an integer"));
            }
            // `char` buffers are bytes (`c_char` is `i8` or `u8` depending on
            // the target).
            let elem = match self.resolve(pointee) {
                Type::Void | Type::Char => "u8".to_owned(),
                _ => rust_type(pointee),
            };
            how[p] = Arg::Slice {
                mutable: !is_const,
                elem,
            };
            how[l] = Arg::Len(p);
        }
        for out in &w.outs {
            let i = find(out)?;
            let Type::Pointer {
                pointee,
                is_const: false,
            } = self.resolve(&params[i].ty)
            else {
                return Err(format!("`{out}` is not a mutable pointer"));
            };
            if **pointee == Type::Void {
                return Err(format!("`{out}` points to `void`"));
            }
            how[i] = Arg::Out(rust_type(pointee));
        }

        let mut plan = Plan {
            params: String::new(),
            setup: String::new(),
            args: Vec::new(),
            ret_ty: String::new(),
            ret_expr: String::new(),
            uses_ret: true,
        };
        let mut outs: Vec<(String, String)> = Vec::new();
        for (i, a) in how.iter().enumerate() {
            let n = ident(&names[i]);
            match a {
                Arg::Plain => {
                    let _ = write!(plan.params, ", {n}: {}", rust_type(&params[i].ty));
                    plan.args.push(n);
                }
                Arg::Str => {
                    let _ = write!(plan.params, ", {n}: &::core::ffi::CStr");
                    plan.args.push(format!("{n}.as_ptr()"));
                }
                Arg::Slice { mutable, elem } => {
                    let (r, m) = if *mutable {
                        ("&mut ", "as_mut_ptr")
                    } else {
                        ("&", "as_ptr")
                    };
                    let _ = write!(plan.params, ", {n}: {r}[{elem}]");
                    plan.args.push(format!("{n}.{m}().cast()"));
                }
                Arg::Len(p) => {
                    let slice = ident(&names[*p]);
                    let ty = rust_type(&params[i].ty);
                    if ty == "usize" {
                        let _ = writeln!(plan.setup, "        let {n} = {slice}.len();");
                    } else {
                        let _ = writeln!(
                            plan.setup,
                            "        let {n} = <{ty}>::try_from({slice}.len()).expect(\"`{slice}` is too long\");"
                        );
                    }
                    plan.args.push(n);
                }
                Arg::Out(ty) => {
                    let _ = writeln!(
                        plan.setup,
                        "        // SAFETY: generated types are valid when zeroed.\n        \
                         let mut {n}: {ty} = unsafe {{ ::core::mem::zeroed() }};"
                    );
                    plan.args.push(format!("&raw mut {n}"));
                    outs.push((n, ty.clone()));
                }
            }
        }

        let raw = (f.ret != Type::Void).then(|| rust_type(&f.ret));
        // `(value, out...)` as a type and an expression.
        let tuple = |with_ret: bool| {
            let mut tys = Vec::new();
            let mut exprs = Vec::new();
            if with_ret && let Some(r) = &raw {
                tys.push(r.clone());
                exprs.push("ret".to_owned());
            }
            for (n, t) in &outs {
                tys.push(t.clone());
                exprs.push(n.clone());
            }
            match tys.len() {
                1 => (tys.remove(0), exprs.remove(0)),
                _ => (
                    format!("({})", tys.join(", ")),
                    format!("({})", exprs.join(", ")),
                ),
            }
        };
        let code = || -> Result<String, String> {
            match self.resolve(&f.ret) {
                t if is_integer(t) => Ok("ret".to_owned()),
                Type::Named(n) if self.enums.contains_key(n.as_str()) => Ok("ret.0".to_owned()),
                _ => Err("ret= needs an integer return type".to_owned()),
            }
        };
        match w.ret {
            None => {
                let (ty, expr) = tuple(true);
                plan.uses_ret = raw.is_some();
                (plan.ret_ty, plan.ret_expr) = (ty, expr);
            }
            Some(Ret::NonZero) => {
                let c = code()?;
                let (ok, expr) = tuple(false);
                let r = raw.clone().unwrap_or_default();
                plan.ret_ty = format!("::core::result::Result<{ok}, {r}>");
                plan.ret_expr = format!(
                    "if {c} == 0 {{ ::core::result::Result::Ok({expr}) }} else {{ ::core::result::Result::Err(ret) }}"
                );
            }
            Some(Ret::Negative) => {
                let c = code()?;
                let (ok, expr) = tuple(true);
                let r = raw.clone().unwrap_or_default();
                plan.ret_ty = format!("::core::result::Result<{ok}, {r}>");
                plan.ret_expr = format!(
                    "if {c} >= 0 {{ ::core::result::Result::Ok({expr}) }} else {{ ::core::result::Result::Err(ret) }}"
                );
            }
            Some(Ret::Errno) => {
                let c = code()?;
                let (ok, expr) = tuple(true);
                plan.ret_ty = format!("::std::io::Result<{ok}>");
                plan.ret_expr = format!(
                    "if {c} >= 0 {{ ::core::result::Result::Ok({expr}) }} else {{ ::core::result::Result::Err(::dlsym::__private::errno_error()) }}"
                );
            }
            Some(Ret::StaticStr) => {
                if !self.is_c_string(&f.ret) {
                    return Err("ret=static-str needs a `const char *` return type".into());
                }
                let s = "::core::option::Option<&::core::ffi::CStr>";
                let conv = "if ret.is_null() { ::core::option::Option::None } else {\n            \
                            // SAFETY: the library returns a NUL-terminated string that lives as\n            \
                            // long as it does (`ret=static-str`), and `self` borrows the library.\n            \
                            ::core::option::Option::Some(unsafe { ::core::ffi::CStr::from_ptr(ret) })\n        }";
                if outs.is_empty() {
                    (plan.ret_ty, plan.ret_expr) = (s.into(), conv.into());
                } else {
                    let (ty, expr) = tuple(false);
                    plan.ret_ty = format!("({s}, {ty})");
                    plan.ret_expr = format!("({conv}, {expr})");
                }
            }
        }
        Ok(plan)
    }

    /// Whether `ty` is `const char *` (or a typedef of `char`).
    pub(super) fn is_c_string(&self, ty: &Type) -> bool {
        matches!(self.resolve(ty), Type::Pointer { pointee, is_const: true }
            if matches!(self.resolve(pointee), Type::Char | Type::SChar | Type::UChar))
    }

    /// The body of a wrapper calling `call` (a raw method call returning
    /// `Option` if `optional`), in an `unsafe` block if `unsafe_call`.
    pub(super) fn wrapper_body(
        plan: &Plan,
        call: &str,
        optional: bool,
        unsafe_call: bool,
    ) -> String {
        if plan.setup.is_empty() && (plan.ret_expr == "ret" || !plan.uses_ret) {
            // Nothing to convert: forward directly.
            let call = if unsafe_call {
                format!(
                    "        // SAFETY: the requirements are the caller's (or declared met).\n        \
                     unsafe {{ {call} }}\n"
                )
            } else {
                format!("        {call}\n")
            };
            return call;
        }
        let q = if optional { "?" } else { "" };
        let call = if unsafe_call {
            format!("unsafe {{ {call} }}{q}")
        } else {
            format!("{call}{q}")
        };
        let mut body = plan.setup.clone();
        if unsafe_call {
            body.push_str("        // SAFETY: the arguments are converted from valid Rust values; the\n        \
                           // remaining requirements are the caller's (or `safe` asserts them).\n");
        }
        let expr = if optional {
            format!("::core::option::Option::Some({})", plan.ret_expr)
        } else {
            plan.ret_expr.clone()
        };
        if plan.uses_ret {
            let _ = write!(body, "        let ret = {call};\n        {expr}\n");
        } else {
            let _ = write!(body, "        {call};\n        {expr}\n");
        }
        body
    }

    /// The return type of a wrapper (` -> T`, or nothing for `()`).
    pub(super) fn wrapper_ret(plan: &Plan, optional: bool) -> String {
        match (plan.ret_ty.as_str(), optional) {
            ("()", false) | ("", false) => String::new(),
            ("()", true) | ("", true) => " -> ::core::option::Option<()>".into(),
            (t, false) => format!(" -> {t}"),
            (t, true) => format!(" -> ::core::option::Option<{t}>"),
        }
    }

    /// Emits `{name}_rs` wrappers on the bindings struct for functions with
    /// [`Options::wraps`] annotations.
    pub(super) fn wrappers(&mut self, header: &Header, opts: &Options) {
        if opts.wraps.is_empty() {
            return;
        }
        let taken: std::collections::HashSet<&str> = header
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Function(f) => Some(f.name.as_str()),
                Item::Var(v) => Some(v.name.as_str()),
                Item::MacroFn(m) => Some(m.name.as_str()),
                _ => None,
            })
            .collect();
        let mut methods = String::new();
        for item in &header.items {
            let Item::Function(f) = item else { continue };
            let Some(w) = Wrap::find(&opts.wraps, &f.name) else {
                continue;
            };
            let name = format!("{}_rs", f.name);
            if f.variadic || taken.contains(name.as_str()) {
                let _ = writeln!(
                    self.out,
                    "\n// skipped wrapper `{name}`: variadic or name taken"
                );
                continue;
            }
            let plan = match self.plan(f, 0, &w) {
                Ok(p) => p,
                Err(why) => {
                    let _ = writeln!(self.out, "\n// skipped wrapper `{name}`: {why}");
                    continue;
                }
            };
            let optional = opts.optional.iter().any(|p| glob(p, &f.name));
            let raw_safe = !f.variadic && opts.safe.iter().any(|p| glob(p, &f.name));
            let call = format!("self.{}({})", ident(&f.name), plan.args.join(", "));
            let body = Self::wrapper_body(&plan, &call, optional, !raw_safe);
            let ret = Self::wrapper_ret(&plan, optional);
            let (qual, safety) = if w.safe {
                ("", String::new())
            } else {
                (
                    "unsafe ",
                    format!(
                        "    ///\n    /// # Safety\n    ///\n    /// Same as calling `{}`, except for the converted arguments.\n",
                        f.name
                    ),
                )
            };
            let _ = write!(
                methods,
                "    /// [`{0}`](Self::{0}) with Rust types.\n\
                 {safety}    \
                 #[allow(clippy::too_many_arguments)]\n    \
                 pub {qual}fn {1}(&self{2}){ret} {{\n{body}    }}\n\n",
                ident(&f.name),
                ident(&name),
                plan.params,
            );
        }
        if !methods.is_empty() {
            let _ = write!(
                self.out,
                "\nimpl {} {{\n{}}}\n",
                ident(&opts.struct_name),
                methods.trim_end_matches('\n').to_owned() + "\n"
            );
        }
    }
}

fn is_integer(t: &Type) -> bool {
    matches!(
        t,
        Type::Char
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specs() {
        let w = Wrap::parse("gz*: str, slice=buf/len ,out=n,ret=errno,safe").unwrap();
        assert_eq!(w.pattern, "gz*");
        assert!(w.all_strings && w.safe);
        assert_eq!(w.slices, [("buf".to_owned(), "len".to_owned())]);
        assert_eq!(w.outs, ["n"]);
        assert_eq!(w.ret, Some(Ret::Errno));
        assert!(Wrap::parse("gzopen").is_err());
        assert!(Wrap::parse("gzopen:slice=buf").is_err());
        assert!(Wrap::parse("gzopen:ret=maybe").is_err());
        assert!(Wrap::parse("gzopen:frobnicate").is_err());

        let all = [
            Wrap::parse("gz*:str").unwrap(),
            Wrap::parse("gzread:slice=buf/len,ret=negative").unwrap(),
        ];
        let m = Wrap::find(&all, "gzread").unwrap();
        assert!(m.all_strings);
        assert_eq!(m.ret, Some(Ret::Negative));
        assert!(Wrap::find(&all, "deflate").is_none());
    }
}
