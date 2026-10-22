//! Intermediate representation of the declarations found in C headers.
//!
//! Names in the IR are final Rust names: the front end resolves typedefs,
//! anonymous types and tag/typedef collisions before handing items over.

/// Every declaration extracted from a set of headers, in emission order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Header {
    /// Declarations, in source order (types first as needed).
    pub items: Vec<Item>,
}

/// A top-level declaration.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Item {
    /// A function prototype.
    Function(Function),
    /// A global variable.
    Var(Var),
    /// A struct or union.
    Record(Record),
    /// An enum.
    Enum(Enum),
    /// A typedef.
    Typedef(Typedef),
    /// A constant (from a `#define`).
    Const(Const),
}

/// A calling convention.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Abi {
    /// The platform's C calling convention.
    #[default]
    C,
    /// `__stdcall` (32-bit Windows).
    Stdcall,
    /// `__fastcall` (32-bit Windows).
    Fastcall,
    /// `__thiscall` (32-bit Windows).
    Thiscall,
    /// `__vectorcall`.
    Vectorcall,
}

impl Abi {
    /// The ABI string used in `extern "..."`.
    pub fn as_str(self) -> &'static str {
        match self {
            Abi::C => "C",
            Abi::Stdcall => "stdcall",
            Abi::Fastcall => "fastcall",
            Abi::Thiscall => "thiscall",
            Abi::Vectorcall => "vectorcall",
        }
    }
}

/// A function prototype.
#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    /// The symbol name.
    pub name: String,
    /// The parameters, in order.
    pub params: Vec<Param>,
    /// The return type.
    pub ret: Type,
    /// Whether the function takes `...`.
    pub variadic: bool,
    /// The calling convention.
    pub abi: Abi,
    /// The documentation comment, if any.
    pub doc: Option<String>,
}

/// A function parameter.
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    /// The parameter name, if the prototype gives one.
    pub name: Option<String>,
    /// The parameter type.
    pub ty: Type,
}

/// A global variable exported by the library.
#[derive(Debug, Clone, PartialEq)]
pub struct Var {
    /// The symbol name.
    pub name: String,
    /// The variable's type.
    pub ty: Type,
    /// Whether the variable is `const`.
    pub is_const: bool,
    /// The documentation comment, if any.
    pub doc: Option<String>,
}

/// Whether a record is a struct or a union.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordKind {
    /// `struct`
    Struct,
    /// `union`
    Union,
}

/// A struct or union.
#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    /// The Rust name.
    pub name: String,
    /// Struct or union.
    pub kind: RecordKind,
    /// The definition, or `None` for an opaque (incomplete) type.
    pub body: Option<RecordBody>,
    /// The documentation comment, if any.
    pub doc: Option<String>,
}

/// The layout and fields of a complete record.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordBody {
    /// Size in bytes.
    pub size: u64,
    /// Alignment in bytes.
    pub align: u64,
    /// Fields, in declaration order.
    pub fields: Vec<Field>,
}

/// A record field.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    /// The Rust name.
    pub name: String,
    /// The field type.
    pub ty: Type,
    /// Offset from the start of the record, in bits.
    pub offset_bits: u64,
    /// Width in bits for bitfields.
    pub bit_width: Option<u32>,
    /// Size and alignment of the field's type in bytes, as computed by the
    /// C compiler (unused for bitfields).
    pub layout: (u64, u64),
    /// The documentation comment, if any.
    pub doc: Option<String>,
}

/// An enum.
#[derive(Debug, Clone, PartialEq)]
pub struct Enum {
    /// The Rust name, or `None` for an anonymous enum whose constants are
    /// emitted with the underlying integer type.
    pub name: Option<String>,
    /// The underlying integer type.
    pub repr: Type,
    /// The constants.
    pub variants: Vec<Variant>,
    /// The documentation comment, if any.
    pub doc: Option<String>,
}

/// An enum constant.
#[derive(Debug, Clone, PartialEq)]
pub struct Variant {
    /// The constant's name.
    pub name: String,
    /// Its value.
    pub value: i128,
    /// The documentation comment, if any.
    pub doc: Option<String>,
}

/// A typedef.
#[derive(Debug, Clone, PartialEq)]
pub struct Typedef {
    /// The Rust name.
    pub name: String,
    /// The aliased type.
    pub ty: Type,
    /// The documentation comment, if any.
    pub doc: Option<String>,
}

/// A constant.
#[derive(Debug, Clone, PartialEq)]
pub struct Const {
    /// The constant's name.
    pub name: String,
    /// Its value.
    pub value: ConstValue,
    /// The documentation comment, if any.
    pub doc: Option<String>,
}

/// A constant's value.
#[derive(Debug, Clone, PartialEq)]
pub enum ConstValue {
    /// An integer of the given type.
    Int(i128, Type),
    /// A floating-point number of the given type.
    Float(f64, Type),
    /// A string (without the terminating NUL).
    Str(Vec<u8>),
}

/// A C type.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Type {
    /// `void`
    Void,
    /// `_Bool` / `bool`
    Bool,
    /// `char`
    Char,
    /// `signed char`
    SChar,
    /// `unsigned char`
    UChar,
    /// `short`
    Short,
    /// `unsigned short`
    UShort,
    /// `int`
    Int,
    /// `unsigned int`
    UInt,
    /// `long`
    Long,
    /// `unsigned long`
    ULong,
    /// `long long`
    LongLong,
    /// `unsigned long long`
    ULongLong,
    /// `float`
    Float,
    /// `double`
    Double,
    /// A fixed-width integer (`int8_t` … `uint64_t`, `__int128`), as
    /// `(bits, signed)`.
    Fixed(u32, bool),
    /// `size_t` / `uintptr_t`
    USize,
    /// `ssize_t` / `ptrdiff_t` / `intptr_t`
    ISize,
    /// A pointer.
    Pointer {
        /// The pointed-to type.
        pointee: Box<Type>,
        /// Whether the pointee is `const`-qualified.
        is_const: bool,
    },
    /// A fixed-size array.
    Array {
        /// The element type.
        elem: Box<Type>,
        /// The number of elements.
        len: u64,
    },
    /// A function pointer (nullable).
    FnPtr {
        /// Parameter types.
        params: Vec<Type>,
        /// Return type.
        ret: Box<Type>,
        /// Whether it takes `...`.
        variadic: bool,
        /// Calling convention.
        abi: Abi,
    },
    /// A typedef, struct, union or enum, by its Rust name.
    Named(String),
    /// A type with no Rust equivalent (e.g. `long double`); items using it
    /// are skipped.
    Unsupported(String),
}

impl Type {
    /// Convenience constructor for `T *`.
    pub fn ptr(pointee: Type) -> Self {
        Type::Pointer {
            pointee: Box::new(pointee),
            is_const: false,
        }
    }

    /// Convenience constructor for `const T *`.
    pub fn const_ptr(pointee: Type) -> Self {
        Type::Pointer {
            pointee: Box::new(pointee),
            is_const: true,
        }
    }

    /// Returns the first unsupported type this type contains, if any.
    pub fn unsupported(&self) -> Option<&str> {
        match self {
            Type::Unsupported(what) => Some(what),
            Type::Pointer { pointee, .. } => pointee.unsupported(),
            Type::Array { elem, .. } => elem.unsupported(),
            Type::FnPtr { params, ret, .. } => params
                .iter()
                .chain(std::iter::once(&**ret))
                .find_map(Type::unsupported),
            _ => None,
        }
    }
}
