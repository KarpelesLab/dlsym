//! Minimal libclang bindings, loaded at runtime with this crate's own
//! [`library!`](crate::library) macro.

#![allow(non_snake_case, missing_docs)]
// Every unsafe block here is a sequence of libclang calls on values from a
// live translation unit, all justified by the same invariant.
#![allow(clippy::multiple_unsafe_ops_per_block)]

use std::ffi::{CStr, CString, c_char, c_int, c_longlong, c_uint, c_ulong, c_ulonglong, c_void};
use std::fmt;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::Bindings;

pub(crate) type CXIndex = *mut c_void;
pub(crate) type CXTranslationUnit = *mut c_void;
pub(crate) type CXDiagnostic = *mut c_void;
pub(crate) type CXFile = *mut c_void;
pub(crate) type CXEvalResult = *mut c_void;
pub(crate) type CXVisitor = unsafe extern "C" fn(CXCursor, CXCursor, *mut c_void) -> c_int;

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct CXCursor {
    kind: c_int,
    xdata: c_int,
    data: [*const c_void; 3],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct CXType {
    kind: c_int,
    data: [*mut c_void; 2],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct CXString {
    data: *const c_void,
    private_flags: c_uint,
}

#[repr(C)]
pub(crate) struct CXStringSet {
    strings: *mut CXString,
    count: c_uint,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct CXSourceLocation {
    ptr_data: [*const c_void; 2],
    int_data: c_uint,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct CXSourceRange {
    ptr_data: [*const c_void; 2],
    begin_int_data: c_uint,
    end_int_data: c_uint,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct CXToken {
    int_data: [c_uint; 4],
    ptr_data: *mut c_void,
}

#[repr(C)]
pub(crate) struct CXUnsavedFile {
    filename: *const c_char,
    contents: *const c_char,
    length: c_ulong,
}

crate::library! {
    pub(crate) struct LibClang {
        unsafe extern "C" {
            fn clang_createIndex(exclude_pch: c_int, display_diagnostics: c_int) -> CXIndex;
            fn clang_disposeIndex(index: CXIndex);
            fn clang_parseTranslationUnit2(
                index: CXIndex,
                source: *const c_char,
                args: *const *const c_char,
                num_args: c_int,
                unsaved: *mut CXUnsavedFile,
                num_unsaved: c_uint,
                options: c_uint,
                out: *mut CXTranslationUnit,
            ) -> c_int;
            fn clang_disposeTranslationUnit(tu: CXTranslationUnit);
            fn clang_getNumDiagnostics(tu: CXTranslationUnit) -> c_uint;
            fn clang_getDiagnostic(tu: CXTranslationUnit, index: c_uint) -> CXDiagnostic;
            fn clang_getDiagnosticSeverity(diagnostic: CXDiagnostic) -> c_int;
            fn clang_formatDiagnostic(diagnostic: CXDiagnostic, options: c_uint) -> CXString;
            fn clang_defaultDiagnosticDisplayOptions() -> c_uint;
            fn clang_disposeDiagnostic(diagnostic: CXDiagnostic);
            fn clang_getTranslationUnitCursor(tu: CXTranslationUnit) -> CXCursor;
            #[optional]
            fn clang_getTranslationUnitTargetInfo(tu: CXTranslationUnit) -> *mut c_void;
            #[optional]
            fn clang_TargetInfo_getTriple(info: *mut c_void) -> CXString;
            #[optional]
            fn clang_TargetInfo_dispose(info: *mut c_void);
            fn clang_visitChildren(parent: CXCursor, visitor: CXVisitor, data: *mut c_void) -> c_uint;
            fn clang_getCString(s: CXString) -> *const c_char;
            fn clang_disposeString(s: CXString);
            fn clang_getClangVersion() -> CXString;

            fn clang_getCursorKind(cursor: CXCursor) -> c_int;
            fn clang_getCursorSpelling(cursor: CXCursor) -> CXString;
            fn clang_getCursorUSR(cursor: CXCursor) -> CXString;
            fn clang_getCursorType(cursor: CXCursor) -> CXType;
            fn clang_getCursorResultType(cursor: CXCursor) -> CXType;
            fn clang_getCursorLinkage(cursor: CXCursor) -> c_int;
            fn clang_getCursorSemanticParent(cursor: CXCursor) -> CXCursor;
            fn clang_getCXXAccessSpecifier(cursor: CXCursor) -> c_int;
            fn clang_CXXMethod_isStatic(cursor: CXCursor) -> c_uint;
            fn clang_CXXMethod_isVirtual(cursor: CXCursor) -> c_uint;
            fn clang_CXXMethod_isConst(cursor: CXCursor) -> c_uint;
            #[optional]
            fn clang_CXXRecord_isAbstract(cursor: CXCursor) -> c_uint;
            fn clang_CXXConstructor_isCopyConstructor(cursor: CXCursor) -> c_uint;
            fn clang_CXXConstructor_isMoveConstructor(cursor: CXCursor) -> c_uint;
            #[optional]
            fn clang_Cursor_getMangling(cursor: CXCursor) -> CXString;
            #[optional]
            fn clang_Cursor_getCXXManglings(cursor: CXCursor) -> *mut CXStringSet;
            #[optional]
            fn clang_disposeStringSet(set: *mut CXStringSet);
            #[optional]
            fn clang_getCursorExceptionSpecificationType(cursor: CXCursor) -> c_int;
            #[optional]
            fn clang_CXXMethod_isDeleted(cursor: CXCursor) -> c_uint;
            #[optional]
            fn clang_Cursor_isFunctionInlined(cursor: CXCursor) -> c_uint;
            #[optional]
            fn clang_EnumDecl_isScoped(cursor: CXCursor) -> c_uint;
            fn clang_getCursorDefinition(cursor: CXCursor) -> CXCursor;
            fn clang_getCanonicalCursor(cursor: CXCursor) -> CXCursor;
            fn clang_getCursorLocation(cursor: CXCursor) -> CXSourceLocation;
            fn clang_getCursorExtent(cursor: CXCursor) -> CXSourceRange;
            fn clang_isCursorDefinition(cursor: CXCursor) -> c_uint;
            fn clang_Cursor_isNull(cursor: CXCursor) -> c_int;
            fn clang_Cursor_isAnonymous(cursor: CXCursor) -> c_uint;
            #[optional]
            fn clang_Cursor_isAnonymousRecordDecl(cursor: CXCursor) -> c_uint;
            fn clang_Cursor_getStorageClass(cursor: CXCursor) -> c_int;
            fn clang_Cursor_getNumArguments(cursor: CXCursor) -> c_int;
            fn clang_Cursor_getArgument(cursor: CXCursor, index: c_uint) -> CXCursor;
            fn clang_Cursor_getOffsetOfField(cursor: CXCursor) -> c_longlong;
            fn clang_Cursor_isBitField(cursor: CXCursor) -> c_uint;
            fn clang_getFieldDeclBitWidth(cursor: CXCursor) -> c_int;
            fn clang_getEnumDeclIntegerType(cursor: CXCursor) -> CXType;
            fn clang_getEnumConstantDeclValue(cursor: CXCursor) -> c_longlong;
            fn clang_getEnumConstantDeclUnsignedValue(cursor: CXCursor) -> c_ulonglong;
            fn clang_getTypedefDeclUnderlyingType(cursor: CXCursor) -> CXType;
            fn clang_Cursor_getRawCommentText(cursor: CXCursor) -> CXString;
            fn clang_Cursor_isMacroFunctionLike(cursor: CXCursor) -> c_uint;
            fn clang_Cursor_isMacroBuiltin(cursor: CXCursor) -> c_uint;
            fn clang_Cursor_Evaluate(cursor: CXCursor) -> CXEvalResult;
            fn clang_getIncludedFile(cursor: CXCursor) -> CXFile;

            fn clang_EvalResult_getKind(result: CXEvalResult) -> c_int;
            fn clang_EvalResult_getAsLongLong(result: CXEvalResult) -> c_longlong;
            fn clang_EvalResult_isUnsignedInt(result: CXEvalResult) -> c_uint;
            fn clang_EvalResult_getAsUnsigned(result: CXEvalResult) -> c_ulonglong;
            fn clang_EvalResult_getAsDouble(result: CXEvalResult) -> f64;
            fn clang_EvalResult_dispose(result: CXEvalResult);

            fn clang_getCanonicalType(ty: CXType) -> CXType;
            fn clang_isPODType(ty: CXType) -> c_uint;
            fn clang_getPointeeType(ty: CXType) -> CXType;
            fn clang_isConstQualifiedType(ty: CXType) -> c_uint;
            fn clang_getTypeDeclaration(ty: CXType) -> CXCursor;
            fn clang_getTypeSpelling(ty: CXType) -> CXString;
            fn clang_Type_getSizeOf(ty: CXType) -> c_longlong;
            fn clang_Type_getAlignOf(ty: CXType) -> c_longlong;
            fn clang_Type_getOffsetOf(ty: CXType, field: *const c_char) -> c_longlong;
            fn clang_getArrayElementType(ty: CXType) -> CXType;
            fn clang_getArraySize(ty: CXType) -> c_longlong;
            fn clang_getResultType(ty: CXType) -> CXType;
            fn clang_getNumArgTypes(ty: CXType) -> c_int;
            fn clang_getArgType(ty: CXType, index: c_uint) -> CXType;
            fn clang_isFunctionTypeVariadic(ty: CXType) -> c_uint;
            fn clang_getFunctionTypeCallingConv(ty: CXType) -> c_int;
            fn clang_Type_getNamedType(ty: CXType) -> CXType;
            #[optional]
            fn clang_Type_getModifiedType(ty: CXType) -> CXType;

            fn clang_getSpellingLocation(
                location: CXSourceLocation,
                file: *mut CXFile,
                line: *mut c_uint,
                column: *mut c_uint,
                offset: *mut c_uint,
            );
            fn clang_getFileName(file: CXFile) -> CXString;
            fn clang_tokenize(
                tu: CXTranslationUnit,
                range: CXSourceRange,
                tokens: *mut *mut CXToken,
                count: *mut c_uint,
            );
            fn clang_getTokenKind(token: CXToken) -> c_int;
            fn clang_getTokenSpelling(tu: CXTranslationUnit, token: CXToken) -> CXString;
            fn clang_disposeTokens(tu: CXTranslationUnit, tokens: *mut CXToken, count: c_uint);
        }
    }
}

// Cursor kinds.
pub(crate) const STRUCT_DECL: c_int = 2;
pub(crate) const UNION_DECL: c_int = 3;
pub(crate) const CLASS_DECL: c_int = 4;
pub(crate) const ENUM_DECL: c_int = 5;
pub(crate) const FIELD_DECL: c_int = 6;
pub(crate) const ENUM_CONSTANT_DECL: c_int = 7;
pub(crate) const FUNCTION_DECL: c_int = 8;
pub(crate) const VAR_DECL: c_int = 9;
pub(crate) const TYPEDEF_DECL: c_int = 20;
pub(crate) const CXX_METHOD: c_int = 21;
pub(crate) const NAMESPACE: c_int = 22;
pub(crate) const LINKAGE_SPEC: c_int = 23;
pub(crate) const CONSTRUCTOR: c_int = 24;
pub(crate) const DESTRUCTOR: c_int = 25;
pub(crate) const TYPE_ALIAS_DECL: c_int = 36;
pub(crate) const CXX_BASE_SPECIFIER: c_int = 44;
pub(crate) const MACRO_DEFINITION: c_int = 501;
pub(crate) const INCLUSION_DIRECTIVE: c_int = 503;

// Type kinds.
pub(crate) const TYPE_INVALID: c_int = 0;
pub(crate) const TYPE_UNEXPOSED: c_int = 1;
pub(crate) const TYPE_VOID: c_int = 2;
pub(crate) const TYPE_BOOL: c_int = 3;
pub(crate) const TYPE_CHAR_U: c_int = 4;
pub(crate) const TYPE_UCHAR: c_int = 5;
pub(crate) const TYPE_CHAR16: c_int = 6;
pub(crate) const TYPE_CHAR32: c_int = 7;
pub(crate) const TYPE_USHORT: c_int = 8;
pub(crate) const TYPE_UINT: c_int = 9;
pub(crate) const TYPE_ULONG: c_int = 10;
pub(crate) const TYPE_ULONGLONG: c_int = 11;
pub(crate) const TYPE_UINT128: c_int = 12;
pub(crate) const TYPE_CHAR_S: c_int = 13;
pub(crate) const TYPE_SCHAR: c_int = 14;
pub(crate) const TYPE_WCHAR: c_int = 15;
pub(crate) const TYPE_SHORT: c_int = 16;
pub(crate) const TYPE_INT: c_int = 17;
pub(crate) const TYPE_LONG: c_int = 18;
pub(crate) const TYPE_LONGLONG: c_int = 19;
pub(crate) const TYPE_INT128: c_int = 20;
pub(crate) const TYPE_FLOAT: c_int = 21;
pub(crate) const TYPE_DOUBLE: c_int = 22;
pub(crate) const TYPE_NULLPTR: c_int = 24;
pub(crate) const TYPE_POINTER: c_int = 101;
pub(crate) const TYPE_LVALUE_REFERENCE: c_int = 103;
pub(crate) const TYPE_RVALUE_REFERENCE: c_int = 104;
pub(crate) const TYPE_RECORD: c_int = 105;
pub(crate) const TYPE_ENUM: c_int = 106;
pub(crate) const TYPE_TYPEDEF: c_int = 107;
pub(crate) const TYPE_FUNCTION_NO_PROTO: c_int = 110;
pub(crate) const TYPE_FUNCTION_PROTO: c_int = 111;
pub(crate) const TYPE_CONSTANT_ARRAY: c_int = 112;
pub(crate) const TYPE_INCOMPLETE_ARRAY: c_int = 114;
pub(crate) const TYPE_ELABORATED: c_int = 119;
pub(crate) const TYPE_ATTRIBUTED: c_int = 163;

// Calling conventions.
pub(crate) const CALLCONV_X86_STDCALL: c_int = 2;
pub(crate) const CALLCONV_X86_FASTCALL: c_int = 3;
pub(crate) const CALLCONV_X86_THISCALL: c_int = 4;
pub(crate) const CALLCONV_X86_VECTORCALL: c_int = 12;

// Misc enums.
pub(crate) const LINKAGE_EXTERNAL: c_int = 4;
pub(crate) const ACCESS_PUBLIC: c_int = 1;
pub(crate) const STORAGE_STATIC: c_int = 3;
pub(crate) const EVAL_INT: c_int = 1;
pub(crate) const EVAL_FLOAT: c_int = 2;
pub(crate) const DIAGNOSTIC_ERROR: c_int = 3;
pub(crate) const TOKEN_LITERAL: c_int = 3;
const CHILD_VISIT_CONTINUE: c_int = 1;

const TU_DETAILED_PREPROCESSING_RECORD: c_uint = 0x01;
const TU_SKIP_FUNCTION_BODIES: c_uint = 0x40;

/// The process-wide libclang, loaded on first use and never unloaded.
///
/// libclang is not safe to unload: on Windows it leaves threads and
/// callbacks behind that crash once its code is gone (on Linux it is linked
/// with `-z nodelete`, so `dlclose` does nothing anyway). Keeping it in a
/// `static` means it is never dropped.
static LIBCLANG: OnceLock<LibClang> = OnceLock::new();

/// Returns libclang, loading it on first use (see [`find`]). Later calls
/// return the same library, whatever `explicit` is.
pub(crate) fn load(explicit: Option<&Path>) -> Result<&'static LibClang, String> {
    if let Some(lib) = LIBCLANG.get() {
        return Ok(lib);
    }
    let lib = find(explicit)?;
    Ok(LIBCLANG.get_or_init(|| lib))
}

/// Locates and loads libclang.
///
/// Search order: `explicit`, then `$LIBCLANG_PATH` (a file or a
/// directory), then `llvm-config --libdir`, then well-known locations for
/// the platform, then the system's default library search path.
fn find(explicit: Option<&Path>) -> Result<LibClang, String> {
    let mut tried = Vec::new();
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(path) = explicit {
        candidates.extend(expand(path));
    }
    if let Some(path) = std::env::var_os("LIBCLANG_PATH") {
        candidates.extend(expand(Path::new(&path)));
    }
    if let Ok(out) = std::process::Command::new("llvm-config")
        .arg("--libdir")
        .output()
        && out.status.success()
    {
        let dir = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        candidates.extend(expand(Path::new(&dir)));
    }
    candidates.extend(well_known());
    for name in file_names() {
        candidates.push(PathBuf::from(name));
    }

    for candidate in candidates {
        // SAFETY: libclang's initialisers are trusted.
        match unsafe { LibClang::open(&candidate) } {
            Ok(lib) => return Ok(lib),
            Err(e) => tried.push(format!("  {}: {e}", candidate.display())),
        }
        if explicit.is_some() && tried.len() == 1 {
            break;
        }
    }
    Err(format!(
        "could not load libclang; set LIBCLANG_PATH or pass --libclang. Tried:\n{}",
        tried.join("\n")
    ))
}

fn file_names() -> Vec<String> {
    if cfg!(windows) {
        vec!["libclang.dll".into()]
    } else if cfg!(target_vendor = "apple") {
        vec!["libclang.dylib".into()]
    } else {
        let mut v = vec!["libclang.so".to_owned(), "libclang.so.1".to_owned()];
        for n in (11..=30).rev() {
            v.push(format!("libclang-{n}.so"));
            v.push(format!("libclang.so.{n}"));
            v.push(format!("libclang-{n}.so.1"));
        }
        v
    }
}

/// A path is either a library file or a directory to look in.
fn expand(path: &Path) -> Vec<PathBuf> {
    if path.is_dir() {
        file_names()
            .into_iter()
            .map(|n| path.join(n))
            .filter(|p| p.exists())
            .collect()
    } else {
        vec![path.to_owned()]
    }
}

fn well_known() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if cfg!(windows) {
        for var in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(pf) = std::env::var_os(var) {
                dirs.push(Path::new(&pf).join("LLVM").join("bin"));
            }
        }
    } else if cfg!(target_vendor = "apple") {
        if let Ok(out) = std::process::Command::new("xcode-select")
            .arg("-p")
            .output()
            && out.status.success()
        {
            let dev = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
            dirs.push(dev.join("Toolchains/XcodeDefault.xctoolchain/usr/lib"));
            dirs.push(dev.join("usr/lib"));
        }
        dirs.push("/Library/Developer/CommandLineTools/usr/lib".into());
        dirs.push(
            "/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/lib"
                .into(),
        );
        dirs.push("/opt/homebrew/opt/llvm/lib".into());
        dirs.push("/usr/local/opt/llvm/lib".into());
    } else {
        // Versioned LLVM trees (Debian/Ubuntu: /usr/lib/llvm-N/lib,
        // Gentoo: /usr/lib/llvm/N/lib64), newest first.
        let mut versioned: Vec<(u32, PathBuf)> = Vec::new();
        for (parent, prefix) in [
            ("/usr/lib", "llvm-"),
            ("/usr/lib/llvm", ""),
            ("/usr/lib64", "llvm"),
        ] {
            let Ok(entries) = std::fs::read_dir(parent) else {
                continue;
            };
            for entry in entries.flatten() {
                let name = entry.file_name();
                let Some(v) = name.to_str().and_then(|n| n.strip_prefix(prefix)) else {
                    continue;
                };
                let Ok(v) = v.parse::<u32>() else { continue };
                for sub in ["lib", "lib64"] {
                    versioned.push((v, entry.path().join(sub)));
                }
            }
        }
        versioned.sort_by_key(|v| std::cmp::Reverse(v.0));
        dirs.extend(versioned.into_iter().map(|(_, p)| p));
        dirs.extend(
            [
                "/usr/lib64",
                "/usr/lib",
                "/usr/local/lib",
                "/usr/lib/x86_64-linux-gnu",
                "/usr/lib/aarch64-linux-gnu",
            ]
            .map(PathBuf::from),
        );
    }
    dirs.iter()
        .flat_map(|d| expand(d))
        .filter(|p| p.exists())
        .collect()
}

/// Turns a `CXString` into an owned `String`, disposing it.
fn take(lib: &LibClang, s: CXString) -> String {
    // SAFETY: `s` was returned by libclang and is disposed exactly once.
    unsafe {
        let ptr = lib.clang_getCString(s);
        let out = if ptr.is_null() {
            String::new()
        } else {
            CStr::from_ptr(ptr).to_string_lossy().into_owned()
        };
        lib.clang_disposeString(s);
        out
    }
}

/// An index plus a parsed translation unit.
pub(crate) struct Tu<'a> {
    lib: &'a LibClang,
    index: CXIndex,
    tu: CXTranslationUnit,
}

/// A diagnostic produced while parsing.
pub(crate) struct Diagnostic {
    pub(crate) is_error: bool,
    pub(crate) message: String,
}

impl<'a> Tu<'a> {
    /// Parses `file` (possibly provided in memory through `unsaved`).
    pub(crate) fn parse(
        lib: &'a LibClang,
        file: &str,
        args: &[String],
        unsaved: &[(String, String)],
        detailed: bool,
    ) -> Result<Self, String> {
        let file_c = CString::new(file).map_err(|e| e.to_string())?;
        let args_c: Vec<CString> = args
            .iter()
            .map(|a| CString::new(a.as_str()))
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?;
        let args_p: Vec<*const c_char> = args_c.iter().map(|a| a.as_ptr()).collect();
        let unsaved_c: Vec<(CString, CString, usize)> = unsaved
            .iter()
            .map(|(name, contents)| {
                Ok((
                    CString::new(name.as_str())?,
                    CString::new(contents.as_str())?,
                    contents.len(),
                ))
            })
            .collect::<Result<_, std::ffi::NulError>>()
            .map_err(|e| e.to_string())?;
        let mut unsaved_raw: Vec<CXUnsavedFile> = unsaved_c
            .iter()
            .map(|(n, c, len)| CXUnsavedFile {
                filename: n.as_ptr(),
                contents: c.as_ptr(),
                length: *len as c_ulong,
            })
            .collect();

        let mut options = TU_SKIP_FUNCTION_BODIES;
        if detailed {
            options |= TU_DETAILED_PREPROCESSING_RECORD;
        }
        // SAFETY: no preconditions.
        let index = unsafe { lib.clang_createIndex(0, 0) };
        let mut tu = std::ptr::null_mut();
        // SAFETY: every pointer is valid for the duration of the call and
        // the counts match the arrays.
        let code = unsafe {
            lib.clang_parseTranslationUnit2(
                index,
                file_c.as_ptr(),
                args_p.as_ptr(),
                args_p.len() as c_int,
                unsaved_raw.as_mut_ptr(),
                unsaved_raw.len() as c_uint,
                options,
                &mut tu,
            )
        };
        if code != 0 || tu.is_null() {
            // SAFETY: `index` was created above and is not used again.
            unsafe { lib.clang_disposeIndex(index) };
            return Err(format!(
                "libclang failed to parse {file} (error code {code})"
            ));
        }
        Ok(Tu { lib, index, tu })
    }

    pub(crate) fn diagnostics(&self) -> Vec<Diagnostic> {
        let lib = self.lib;
        // SAFETY: the TU is live; every diagnostic is disposed once.
        unsafe {
            let n = lib.clang_getNumDiagnostics(self.tu);
            let opts = lib.clang_defaultDiagnosticDisplayOptions();
            (0..n)
                .map(|i| {
                    let d = lib.clang_getDiagnostic(self.tu, i);
                    let is_error = lib.clang_getDiagnosticSeverity(d) >= DIAGNOSTIC_ERROR;
                    let message = take(lib, lib.clang_formatDiagnostic(d, opts));
                    lib.clang_disposeDiagnostic(d);
                    Diagnostic { is_error, message }
                })
                .collect()
        }
    }

    /// The target triple clang parsed for, if libclang can tell.
    pub(crate) fn target_triple(&self) -> Option<String> {
        let lib = self.lib;
        // SAFETY: the TU is live; the target info is disposed once.
        unsafe {
            let info = lib.clang_getTranslationUnitTargetInfo(self.tu)?;
            if info.is_null() {
                return None;
            }
            let triple = lib.clang_TargetInfo_getTriple(info).map(|s| take(lib, s));
            lib.clang_TargetInfo_dispose(info);
            triple
        }
    }

    pub(crate) fn cursor(&self) -> Cursor<'_> {
        // SAFETY: the TU is live.
        Cursor::new(self, unsafe {
            self.lib.clang_getTranslationUnitCursor(self.tu)
        })
    }

    /// Returns the spelling of every token in `cursor`'s extent, with a flag
    /// set for literal tokens.
    pub(crate) fn tokens(&self, cursor: Cursor<'_>) -> Vec<(bool, String)> {
        let lib = self.lib;
        let mut tokens = std::ptr::null_mut();
        let mut count = 0;
        // SAFETY: the TU and cursor are live; the tokens are disposed below.
        unsafe {
            lib.clang_tokenize(
                self.tu,
                lib.clang_getCursorExtent(cursor.raw),
                &mut tokens,
                &mut count,
            );
            if tokens.is_null() {
                return Vec::new();
            }
            let out = std::slice::from_raw_parts(tokens, count as usize)
                .iter()
                .map(|t| {
                    (
                        lib.clang_getTokenKind(*t) == TOKEN_LITERAL,
                        take(lib, lib.clang_getTokenSpelling(self.tu, *t)),
                    )
                })
                .collect();
            lib.clang_disposeTokens(self.tu, tokens, count);
            out
        }
    }
}

impl Drop for Tu<'_> {
    fn drop(&mut self) {
        // SAFETY: both were created in `parse` and are disposed once.
        unsafe {
            self.lib.clang_disposeTranslationUnit(self.tu);
            self.lib.clang_disposeIndex(self.index);
        }
    }
}

/// Returns libclang's version string.
pub(crate) fn version(lib: &LibClang) -> String {
    // SAFETY: no preconditions.
    take(lib, unsafe { lib.clang_getClangVersion() })
}

/// A cursor into a live translation unit.
#[derive(Clone, Copy)]
pub(crate) struct Cursor<'tu> {
    lib: &'tu LibClang,
    raw: CXCursor,
    _tu: PhantomData<&'tu Tu<'tu>>,
}

/// A type from a live translation unit.
#[derive(Clone, Copy)]
pub(crate) struct Type<'tu> {
    lib: &'tu LibClang,
    raw: CXType,
    _tu: PhantomData<&'tu Tu<'tu>>,
}

/// A source location, resolved to a file name and offset.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Location {
    pub(crate) file: String,
    pub(crate) line: u32,
    pub(crate) offset: u32,
}

impl fmt::Debug for Cursor<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Cursor({} {:?})", self.kind(), self.spelling())
    }
}

impl<'tu> Cursor<'tu> {
    fn new(tu: &'tu Tu<'_>, raw: CXCursor) -> Self {
        Cursor {
            lib: tu.lib,
            raw,
            _tu: PhantomData,
        }
    }

    fn wrap(self, raw: CXCursor) -> Self {
        Cursor { raw, ..self }
    }

    fn ty(self, raw: CXType) -> Type<'tu> {
        Type {
            lib: self.lib,
            raw,
            _tu: PhantomData,
        }
    }

    pub(crate) fn kind(self) -> c_int {
        // SAFETY: the cursor belongs to a live TU (all methods below).
        unsafe { self.lib.clang_getCursorKind(self.raw) }
    }

    pub(crate) fn is_null(self) -> bool {
        // SAFETY: see `kind`.
        unsafe { self.lib.clang_Cursor_isNull(self.raw) != 0 }
    }

    pub(crate) fn spelling(self) -> String {
        // SAFETY: see `kind`.
        take(self.lib, unsafe {
            self.lib.clang_getCursorSpelling(self.raw)
        })
    }

    pub(crate) fn usr(self) -> String {
        // SAFETY: see `kind`.
        take(self.lib, unsafe { self.lib.clang_getCursorUSR(self.raw) })
    }

    pub(crate) fn children(self) -> Vec<Cursor<'tu>> {
        unsafe extern "C" fn visit(
            cursor: CXCursor,
            _parent: CXCursor,
            data: *mut c_void,
        ) -> c_int {
            // SAFETY: `data` is the `Vec` passed below, exclusively borrowed
            // for the duration of `clang_visitChildren`.
            unsafe { (*data.cast::<Vec<CXCursor>>()).push(cursor) };
            CHILD_VISIT_CONTINUE
        }
        let mut raw: Vec<CXCursor> = Vec::new();
        // SAFETY: see `kind`; `visit` matches `CXVisitor` and `raw`
        // outlives the call.
        unsafe {
            self.lib
                .clang_visitChildren(self.raw, visit, (&raw mut raw).cast());
        }
        raw.into_iter().map(|c| self.wrap(c)).collect()
    }

    pub(crate) fn cursor_type(self) -> Type<'tu> {
        // SAFETY: see `kind`.
        self.ty(unsafe { self.lib.clang_getCursorType(self.raw) })
    }

    pub(crate) fn result_type(self) -> Type<'tu> {
        // SAFETY: see `kind`.
        self.ty(unsafe { self.lib.clang_getCursorResultType(self.raw) })
    }

    pub(crate) fn typedef_underlying(self) -> Type<'tu> {
        // SAFETY: see `kind`.
        self.ty(unsafe { self.lib.clang_getTypedefDeclUnderlyingType(self.raw) })
    }

    pub(crate) fn enum_integer_type(self) -> Type<'tu> {
        // SAFETY: see `kind`.
        self.ty(unsafe { self.lib.clang_getEnumDeclIntegerType(self.raw) })
    }

    pub(crate) fn enum_value(self, unsigned: bool) -> i128 {
        // SAFETY: see `kind`.
        unsafe {
            if unsigned {
                self.lib.clang_getEnumConstantDeclUnsignedValue(self.raw) as i128
            } else {
                self.lib.clang_getEnumConstantDeclValue(self.raw) as i128
            }
        }
    }

    pub(crate) fn arguments(self) -> Vec<Cursor<'tu>> {
        // SAFETY: see `kind`.
        unsafe {
            let n = self.lib.clang_Cursor_getNumArguments(self.raw).max(0) as c_uint;
            (0..n)
                .map(|i| self.wrap(self.lib.clang_Cursor_getArgument(self.raw, i)))
                .collect()
        }
    }

    pub(crate) fn linkage(self) -> c_int {
        // SAFETY: see `kind`.
        unsafe { self.lib.clang_getCursorLinkage(self.raw) }
    }

    /// The declaration's semantic parent (namespace, class, or TU).
    pub(crate) fn semantic_parent(self) -> Cursor<'tu> {
        // SAFETY: see `kind`.
        self.wrap(unsafe { self.lib.clang_getCursorSemanticParent(self.raw) })
    }

    /// Whether a C++ member is `public` (or not a member at all).
    pub(crate) fn is_public(self) -> bool {
        // SAFETY: see `kind`.
        let access = unsafe { self.lib.clang_getCXXAccessSpecifier(self.raw) };
        access == 0 || access == ACCESS_PUBLIC
    }

    pub(crate) fn is_static_method(self) -> bool {
        // SAFETY: see `kind`.
        unsafe { self.lib.clang_CXXMethod_isStatic(self.raw) != 0 }
    }

    pub(crate) fn is_virtual_method(self) -> bool {
        // SAFETY: see `kind`.
        unsafe { self.lib.clang_CXXMethod_isVirtual(self.raw) != 0 }
    }

    pub(crate) fn is_const_method(self) -> bool {
        // SAFETY: see `kind`.
        unsafe { self.lib.clang_CXXMethod_isConst(self.raw) != 0 }
    }

    /// Whether this is an abstract class (`true` if libclang cannot tell,
    /// so that nothing tries to construct it).
    pub(crate) fn is_abstract(self) -> bool {
        // SAFETY: see `kind`.
        unsafe { self.lib.clang_CXXRecord_isAbstract(self.raw) }.is_none_or(|v| v != 0)
    }

    /// Whether this is a copy or move constructor.
    pub(crate) fn is_copy_or_move_constructor(self) -> bool {
        // SAFETY: see `kind`.
        unsafe {
            self.lib.clang_CXXConstructor_isCopyConstructor(self.raw) != 0
                || self.lib.clang_CXXConstructor_isMoveConstructor(self.raw) != 0
        }
    }

    /// Whether this is a `= delete`d function (`false` if libclang
    /// cannot tell).
    pub(crate) fn is_deleted(self) -> bool {
        // SAFETY: see `kind`.
        unsafe { self.lib.clang_CXXMethod_isDeleted(self.raw) }.is_some_and(|v| v != 0)
    }

    /// The symbol name, as the target's assembler spells it (C++ names
    /// mangled, Apple's `_` prefix included). `None` if libclang is too old.
    pub(crate) fn mangling(self) -> Option<String> {
        // SAFETY: see `kind`.
        unsafe { self.lib.clang_Cursor_getMangling(self.raw) }.map(|s| take(self.lib, s))
    }

    /// Every symbol a C++ constructor or destructor is emitted as (base
    /// object variant first). Empty if libclang is too old.
    pub(crate) fn cxx_manglings(self) -> Vec<String> {
        let lib = self.lib;
        // SAFETY: see `kind`.
        let Some(set) = (unsafe { lib.clang_Cursor_getCXXManglings(self.raw) }) else {
            return Vec::new();
        };
        if set.is_null() {
            return Vec::new();
        }
        // SAFETY: `set` is a live string set; its strings are read, then
        // the whole set is disposed once (not its strings individually).
        unsafe {
            let set_ref = &*set;
            let out = (0..set_ref.count as usize)
                .map(|i| {
                    let s = lib.clang_getCString(*set_ref.strings.add(i));
                    if s.is_null() {
                        String::new()
                    } else {
                        CStr::from_ptr(s).to_string_lossy().into_owned()
                    }
                })
                .collect();
            let _ = lib.clang_disposeStringSet(set);
            out
        }
    }

    /// Whether the function is declared not to throw (`noexcept`, `throw()`,
    /// `__attribute__((nothrow))`; `noexcept(expr)` counts as throwing).
    /// `None` if libclang is too old to tell.
    pub(crate) fn is_noexcept(self) -> Option<bool> {
        // SAFETY: see `kind`.
        let spec = unsafe { self.lib.clang_getCursorExceptionSpecificationType(self.raw) }?;
        // DynamicNone (`throw()`), BasicNoexcept, NoThrow.
        Some(matches!(spec, 1 | 4 | 9))
    }

    /// Whether the function is declared `inline` (or `constexpr`); `false`
    /// if libclang cannot tell.
    pub(crate) fn is_inline(self) -> bool {
        // SAFETY: see `kind`.
        unsafe { self.lib.clang_Cursor_isFunctionInlined(self.raw) }.is_some_and(|v| v != 0)
    }

    /// Whether this is an `enum class` (`false` if libclang cannot tell).
    pub(crate) fn is_scoped_enum(self) -> bool {
        // SAFETY: see `kind`.
        unsafe { self.lib.clang_EnumDecl_isScoped(self.raw) }.is_some_and(|v| v != 0)
    }

    pub(crate) fn storage_class(self) -> c_int {
        // SAFETY: see `kind`.
        unsafe { self.lib.clang_Cursor_getStorageClass(self.raw) }
    }

    pub(crate) fn is_definition(self) -> bool {
        // SAFETY: see `kind`.
        unsafe { self.lib.clang_isCursorDefinition(self.raw) != 0 }
    }

    pub(crate) fn definition(self) -> Option<Cursor<'tu>> {
        // SAFETY: see `kind`.
        let def = self.wrap(unsafe { self.lib.clang_getCursorDefinition(self.raw) });
        (!def.is_null()).then_some(def)
    }

    pub(crate) fn canonical(self) -> Cursor<'tu> {
        // SAFETY: see `kind`.
        self.wrap(unsafe { self.lib.clang_getCanonicalCursor(self.raw) })
    }

    /// Whether this is a tag declaration without a name.
    pub(crate) fn is_anonymous(self) -> bool {
        // SAFETY: see `kind`.
        let anon = unsafe { self.lib.clang_Cursor_isAnonymous(self.raw) } != 0;
        anon || {
            let s = self.spelling();
            s.is_empty() || s.contains(' ')
        }
    }

    /// Whether this is a C11 anonymous struct/union member.
    pub(crate) fn is_anonymous_member(self) -> bool {
        // SAFETY: see `kind`.
        unsafe { self.lib.clang_Cursor_isAnonymousRecordDecl(self.raw) }.is_some_and(|v| v != 0)
    }

    pub(crate) fn field_offset_bits(self) -> Option<u64> {
        // SAFETY: see `kind`.
        let off = unsafe { self.lib.clang_Cursor_getOffsetOfField(self.raw) };
        (off >= 0).then_some(off as u64)
    }

    pub(crate) fn bit_width(self) -> Option<u32> {
        // SAFETY: see `kind`.
        unsafe {
            (self.lib.clang_Cursor_isBitField(self.raw) != 0)
                .then(|| self.lib.clang_getFieldDeclBitWidth(self.raw).max(0) as u32)
        }
    }

    pub(crate) fn comment(self) -> Option<String> {
        // SAFETY: see `kind`.
        let raw = take(self.lib, unsafe {
            self.lib.clang_Cursor_getRawCommentText(self.raw)
        });
        clean_comment(&raw)
    }

    pub(crate) fn is_function_like_macro(self) -> bool {
        // SAFETY: see `kind`.
        unsafe { self.lib.clang_Cursor_isMacroFunctionLike(self.raw) != 0 }
    }

    pub(crate) fn is_builtin_macro(self) -> bool {
        // SAFETY: see `kind`.
        unsafe { self.lib.clang_Cursor_isMacroBuiltin(self.raw) != 0 }
    }

    /// Evaluates a variable's initializer.
    pub(crate) fn evaluate(self) -> Option<Evaluated> {
        let lib = self.lib;
        // SAFETY: see `kind`; the result is disposed once.
        unsafe {
            let r = lib.clang_Cursor_Evaluate(self.raw);
            if r.is_null() {
                return None;
            }
            let out = match lib.clang_EvalResult_getKind(r) {
                EVAL_INT if lib.clang_EvalResult_isUnsignedInt(r) != 0 => {
                    Some(Evaluated::Int(lib.clang_EvalResult_getAsUnsigned(r) as i128))
                }
                EVAL_INT => Some(Evaluated::Int(lib.clang_EvalResult_getAsLongLong(r) as i128)),
                EVAL_FLOAT => Some(Evaluated::Float(lib.clang_EvalResult_getAsDouble(r))),
                _ => None,
            };
            lib.clang_EvalResult_dispose(r);
            out
        }
    }

    pub(crate) fn included_file(self) -> Option<String> {
        // SAFETY: see `kind`.
        unsafe {
            let f = self.lib.clang_getIncludedFile(self.raw);
            (!f.is_null()).then(|| take(self.lib, self.lib.clang_getFileName(f)))
        }
    }

    pub(crate) fn location(self) -> Location {
        let mut file = std::ptr::null_mut();
        let (mut line, mut column, mut offset) = (0, 0, 0);
        // SAFETY: see `kind`; the out-pointers are valid.
        unsafe {
            let loc = self.lib.clang_getCursorLocation(self.raw);
            self.lib
                .clang_getSpellingLocation(loc, &mut file, &mut line, &mut column, &mut offset);
        }
        let file = if file.is_null() {
            String::new()
        } else {
            // SAFETY: `file` was just returned by libclang.
            take(self.lib, unsafe { self.lib.clang_getFileName(file) })
        };
        Location { file, line, offset }
    }
}

/// The value of an evaluated constant.
pub(crate) enum Evaluated {
    Int(i128),
    Float(f64),
}

impl fmt::Debug for Type<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Type({} {:?})", self.kind(), self.spelling())
    }
}

impl<'tu> Type<'tu> {
    fn wrap(self, raw: CXType) -> Self {
        Type { raw, ..self }
    }

    pub(crate) fn kind(self) -> c_int {
        self.raw.kind
    }

    pub(crate) fn spelling(self) -> String {
        // SAFETY: the type belongs to a live TU (all methods below).
        take(self.lib, unsafe {
            self.lib.clang_getTypeSpelling(self.raw)
        })
    }

    pub(crate) fn canonical(self) -> Self {
        // SAFETY: see `spelling`.
        self.wrap(unsafe { self.lib.clang_getCanonicalType(self.raw) })
    }

    /// Whether this is a C++ POD type (always true in C).
    pub(crate) fn is_pod(self) -> bool {
        // SAFETY: see `spelling`.
        unsafe { self.lib.clang_isPODType(self.raw) != 0 }
    }

    pub(crate) fn pointee(self) -> Self {
        // SAFETY: see `spelling`.
        self.wrap(unsafe { self.lib.clang_getPointeeType(self.raw) })
    }

    pub(crate) fn is_const(self) -> bool {
        // SAFETY: see `spelling`.
        unsafe { self.lib.clang_isConstQualifiedType(self.raw) != 0 }
    }

    pub(crate) fn declaration(self) -> Cursor<'tu> {
        Cursor {
            lib: self.lib,
            // SAFETY: see `spelling`.
            raw: unsafe { self.lib.clang_getTypeDeclaration(self.raw) },
            _tu: PhantomData,
        }
    }

    pub(crate) fn size(self) -> Option<u64> {
        // SAFETY: see `spelling`.
        let s = unsafe { self.lib.clang_Type_getSizeOf(self.raw) };
        (s >= 0).then_some(s as u64)
    }

    pub(crate) fn align(self) -> Option<u64> {
        // SAFETY: see `spelling`.
        let a = unsafe { self.lib.clang_Type_getAlignOf(self.raw) };
        (a >= 0).then_some(a as u64)
    }

    /// Offset in bits of a (possibly nested anonymous) field.
    pub(crate) fn offset_of(self, field: &str) -> Option<u64> {
        let field = CString::new(field).ok()?;
        // SAFETY: see `spelling`; `field` is a valid C string.
        let off = unsafe { self.lib.clang_Type_getOffsetOf(self.raw, field.as_ptr()) };
        (off >= 0).then_some(off as u64)
    }

    pub(crate) fn element(self) -> Self {
        // SAFETY: see `spelling`.
        self.wrap(unsafe { self.lib.clang_getArrayElementType(self.raw) })
    }

    pub(crate) fn array_len(self) -> Option<u64> {
        // SAFETY: see `spelling`.
        let n = unsafe { self.lib.clang_getArraySize(self.raw) };
        (n >= 0).then_some(n as u64)
    }

    pub(crate) fn result(self) -> Self {
        // SAFETY: see `spelling`.
        self.wrap(unsafe { self.lib.clang_getResultType(self.raw) })
    }

    pub(crate) fn args(self) -> Vec<Self> {
        // SAFETY: see `spelling`.
        unsafe {
            let n = self.lib.clang_getNumArgTypes(self.raw).max(0) as c_uint;
            (0..n)
                .map(|i| self.wrap(self.lib.clang_getArgType(self.raw, i)))
                .collect()
        }
    }

    pub(crate) fn is_variadic(self) -> bool {
        // SAFETY: see `spelling`.
        unsafe { self.lib.clang_isFunctionTypeVariadic(self.raw) != 0 }
    }

    pub(crate) fn calling_convention(self) -> c_int {
        // SAFETY: see `spelling`.
        unsafe { self.lib.clang_getFunctionTypeCallingConv(self.raw) }
    }

    pub(crate) fn named(self) -> Self {
        // SAFETY: see `spelling`.
        self.wrap(unsafe { self.lib.clang_Type_getNamedType(self.raw) })
    }

    /// The type an attributed type modifies, if libclang can tell.
    pub(crate) fn modified(self) -> Option<Self> {
        // SAFETY: see `spelling`.
        unsafe { self.lib.clang_Type_getModifiedType(self.raw) }.map(|t| self.wrap(t))
    }
}

/// Strips comment markers from a raw doc comment.
fn clean_comment(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let mut lines = Vec::new();
    for line in raw.lines() {
        let mut l = line.trim();
        l = l.strip_suffix("*/").unwrap_or(l);
        for prefix in [
            "/**<", "///<", "//!<", "/**", "/*!", "/*", "///", "//!", "//",
        ] {
            if let Some(rest) = l.strip_prefix(prefix) {
                l = rest;
                break;
            }
        }
        let l = l.trim_end();
        let l = match l.trim_start().strip_prefix('*') {
            Some(rest) if !rest.starts_with('/') => rest,
            _ => l,
        };
        lines.push(l.strip_prefix(' ').unwrap_or(l).trim_end().to_owned());
    }
    while lines.first().is_some_and(|l| l.is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    // Remove the common indentation.
    let indent = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    let text: Vec<&str> = lines
        .iter()
        .map(|l| {
            if l.len() >= indent {
                &l[indent..]
            } else {
                l.trim_start()
            }
        })
        .collect();
    let text = text.join("\n");
    (!text.trim().is_empty()).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::clean_comment;

    #[test]
    fn comments() {
        assert_eq!(clean_comment("/** Hello. */").as_deref(), Some("Hello."));
        assert_eq!(
            clean_comment("/**\n * First.\n *\n * Second.\n */").as_deref(),
            Some("First.\n\nSecond.")
        );
        assert_eq!(clean_comment("/// a\n/// b").as_deref(), Some("a\nb"));
        assert_eq!(
            clean_comment("/**< trailing */").as_deref(),
            Some("trailing")
        );
        assert_eq!(clean_comment("/**/"), None);
    }
}
