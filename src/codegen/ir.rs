//! Intermediate representation of the declarations found in a C header.

/// Every declaration extracted from a header (or set of headers).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Header {
    /// Declarations, in source order.
    pub items: Vec<Item>,
}

/// A top-level declaration.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Item {
    /// A function prototype.
    Function(Function),
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
    /// The documentation comment attached to the prototype, if any.
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
    /// `size_t`
    SizeT,
    /// `ssize_t` / `ptrdiff_t`
    SSizeT,
    /// `float`
    Float,
    /// `double`
    Double,
    /// A pointer.
    Pointer {
        /// The pointed-to type.
        pointee: Box<Type>,
        /// Whether the pointee is `const`-qualified.
        is_const: bool,
    },
    /// A typedef, struct, union or enum, referred to by name.
    Named(String),
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
}
