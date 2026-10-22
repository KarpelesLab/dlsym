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
    /// A function-like `#define` that wraps a call to a bound function.
    MacroFn(MacroFn),
}

impl Item {
    /// The Rust type this item defines, if any (records, named enums and
    /// typedefs).
    pub fn type_name(&self) -> Option<&str> {
        match self {
            Item::Record(r) => Some(&r.name),
            Item::Enum(e) => e.name.as_deref(),
            Item::Typedef(t) => Some(&t.name),
            _ => None,
        }
    }
}

/// A function-like macro whose body is a single call to a bound function,
/// such as zlib's
/// `#define deflateInit(strm, level) deflateInit_((strm), (level), ZLIB_VERSION, (int)sizeof(z_stream))`.
///
/// It becomes a method that forwards to the function. Parameter types are
/// taken from the parameters of the called function they are passed to.
#[derive(Debug, Clone, PartialEq)]
pub struct MacroFn {
    /// The macro's name.
    pub name: String,
    /// The macro's parameter names.
    pub params: Vec<String>,
    /// The called function.
    pub target: String,
    /// The arguments passed to it.
    pub args: Vec<MacroArg>,
}

/// One argument of a [`MacroFn`] call.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum MacroArg {
    /// The macro parameter at this index.
    Param(usize),
    /// An integer literal.
    Int(i128),
    /// A string literal (without the terminating NUL).
    Str(Vec<u8>),
    /// A constant (another `#define`), by name.
    Const(String),
    /// `sizeof` a type, by its Rust name.
    SizeOf(String),
}

/// A calling convention.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Abi {
    /// The platform's C calling convention.
    #[default]
    C,
    /// The C calling convention, for functions that may unwind (C++
    /// functions not declared `noexcept`).
    CUnwind,
    /// `__stdcall` (32-bit Windows).
    Stdcall,
    /// `__fastcall` (32-bit Windows).
    Fastcall,
    /// `__thiscall` (32-bit Windows).
    Thiscall,
    /// `__thiscall`, for C++ member functions that may unwind.
    ThiscallUnwind,
    /// `__vectorcall`.
    Vectorcall,
}

impl Abi {
    /// The ABI string used in `extern "..."`.
    pub fn as_str(self) -> &'static str {
        match self {
            Abi::C => "C",
            Abi::CUnwind => "C-unwind",
            Abi::Stdcall => "stdcall",
            Abi::Fastcall => "fastcall",
            Abi::Thiscall => "thiscall",
            Abi::ThiscallUnwind => "thiscall-unwind",
            Abi::Vectorcall => "vectorcall",
        }
    }
}

/// A function prototype.
#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    /// The Rust name (the symbol name for C functions).
    pub name: String,
    /// The symbol name when it differs from `name` (mangled C++ names).
    pub link_name: Option<String>,
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
    /// For C++ member functions, constructors and destructors: the class,
    /// whose `this` pointer is the first parameter.
    pub member: Option<Member>,
}

/// The class a C++ member function belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    /// The class's Rust name.
    pub class: String,
    /// What kind of member it is.
    pub kind: MemberKind,
}

/// The kind of a C++ member function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberKind {
    /// A non-static member function (`const` ones take `*const` `this`).
    Method,
    /// A constructor (the complete-object one), initializing `this`.
    Constructor,
    /// The destructor (the complete-object one).
    Destructor,
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
    /// The Rust name (the symbol name for C variables).
    pub name: String,
    /// The symbol name when it differs from `name` (mangled C++ names).
    pub link_name: Option<String>,
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
    /// For a C++ class that is not trivially copyable: its size and
    /// alignment, so that it can be allocated (its fields stay hidden).
    pub opaque_layout: Option<(u64, u64)>,
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
    /// Whether the field is a `private` or `protected` C++ member: it is
    /// laid out but not `pub` (and gets no bitfield accessors).
    pub is_private: bool,
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

impl Function {
    /// The name the library exports.
    pub fn symbol(&self) -> &str {
        self.link_name.as_deref().unwrap_or(&self.name)
    }
}

impl Var {
    /// The name the library exports.
    pub fn symbol(&self) -> &str {
        self.link_name.as_deref().unwrap_or(&self.name)
    }
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
