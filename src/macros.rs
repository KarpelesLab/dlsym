use std::ffi::OsStr;

use crate::{Library, Result};

/// Structs that bundle a [`Library`] with symbols resolved from it.
///
/// Implemented by the [`library!`](crate::library) macro. Because the struct
/// owns the library, the symbols it stores can never dangle.
pub trait Bindings: Sized {
    /// Resolves every symbol from an already loaded library.
    fn load(library: Library) -> Result<Self>;

    /// Returns the underlying library.
    fn library(&self) -> &Library;

    /// Loads the library at `path` and resolves every symbol.
    ///
    /// If the bound library exports its own `open` function, the generated
    /// method shadows this one; call `<T as Bindings>::open` instead.
    ///
    /// # Safety
    ///
    /// Same as [`Library::open`].
    unsafe fn open(path: impl AsRef<OsStr>) -> Result<Self> {
        // SAFETY: forwarded to the caller.
        Self::load(unsafe { Library::open(path) }?)
    }

    /// Loads the first library in `candidates` that can be opened (see
    /// [`Library::open_first`]) and resolves every symbol.
    ///
    /// # Safety
    ///
    /// Same as [`Library::open`], for every candidate.
    unsafe fn open_first<I>(candidates: I) -> Result<Self>
    where
        I: IntoIterator,
        I::Item: AsRef<OsStr>,
    {
        // SAFETY: forwarded to the caller.
        Self::load(unsafe { Library::open_first(candidates) }?)
    }
}

/// Declares a struct holding a [`Library`] and the symbols resolved from it.
///
/// The body is a list of `unsafe extern "ABI" { ... }` blocks written exactly
/// like Rust 2024 foreign blocks. Writing `unsafe` asserts that every
/// signature is correct, as with a real `extern` block.
///
/// ```no_run
/// use std::ffi::{c_char, c_int, c_long};
/// use dlsym::Bindings;
///
/// dlsym::library! {
///     /// A few functions from the C library.
///     pub struct LibC {
///         unsafe extern "C" {
///             /// `safe` items become safe methods.
///             pub safe fn labs(x: c_long) -> c_long;
///             /// Others become `unsafe` methods.
///             pub fn strlen(s: *const c_char) -> usize;
///             /// Variadic functions are returned as a borrowed [`Symbol`](dlsym::Symbol).
///             pub fn printf(fmt: *const c_char, ...) -> c_int;
///             /// `#[optional]` symbols may be missing; calls return `Option`.
///             #[optional]
///             pub fn strlcpy(dst: *mut c_char, src: *const c_char, n: usize) -> usize;
///             /// `#[link_name]` binds a method to a differently named symbol.
///             #[link_name = "strlen"]
///             pub fn string_length(s: *const c_char) -> usize;
///             /// Data symbols become accessors returning a reference
///             /// (`static`) or a raw pointer (`static mut`).
///             pub static mut environ: *const *const c_char;
///         }
///     }
/// }
///
/// # fn main() -> dlsym::Result<()> {
/// // SAFETY: libc has no unsound initialisers.
/// let libc = unsafe { LibC::open("libc.so.6") }?;
/// assert_eq!(libc.labs(-4), 4);
/// // SAFETY: the argument is a valid C string.
/// assert_eq!(unsafe { libc.strlen(c"abc".as_ptr()) }, 3);
/// // SAFETY: the format string matches the arguments.
/// unsafe { libc.printf().with(|f| f(c"%d\n".as_ptr(), 42)) };
/// if let Some(n) = unsafe { libc.strlcpy(std::ptr::null_mut(), c"x".as_ptr(), 0) } {
///     assert_eq!(n, 1);
/// }
/// # Ok(()) }
/// ```
///
/// Loading fails if any non-`#[optional]` symbol is missing, so a
/// successfully constructed value can call every method without further
/// lookups. Each call is a constant-index array load plus the call itself.
///
/// The generated struct implements [`Bindings`], which provides
/// [`open`](Bindings::open), [`open_first`](Bindings::open_first) and
/// [`load`](Bindings::load).
///
/// Items can carry `#[cfg(..)]`: when disabled, the symbol is not looked up
/// and the method does not exist.
///
/// Expansion time grows with the square of the size of each block, so split
/// very large APIs into several blocks of a few hundred items at most
/// (`dlsym-gen` emits blocks of 64).
///
/// Variadic functions are limited to the `"C"` and `"C-unwind"` ABIs with
/// 1 to 6 fixed arguments (the variadic [`SymbolType`](crate::SymbolType)
/// implementations).
///
/// This is the shape emitted by the `dlsym-gen` tool from C headers.
#[macro_export]
macro_rules! library {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident {
            $( unsafe extern $abi:literal { $($body:tt)* } )*
        }
    ) => {
        $(#[$meta])*
        $vis struct $name {
            __dlsym_library: $crate::Library,
            __dlsym_symbols: $crate::__private::Symbols<{ $name::__DLSYM_COUNT }>,
        }

        impl $name {
            const __DLSYM_COUNT: usize = 0 $(+ $crate::__dlsym_block!{count $abi { $($body)* }})*;
            // References, not arrays: each use of a constant copies its value.
            const __DLSYM_NAMES: &'static [&'static str] = &{
                let mut names = [""; $name::__DLSYM_COUNT];
                let mut index = 0usize;
                $( $crate::__dlsym_block!{names {names, index} $abi { $($body)* }} )*
                let _ = index;
                names
            };
            // Sorting thousands of names at compile time is legitimately long.
            #[allow(long_running_const_eval)]
            const __DLSYM_ORDER: &'static [(u64, usize)] =
                &$crate::__private::sort_order::<{ $name::__DLSYM_COUNT }>($name::__DLSYM_NAMES);

            $( $crate::__dlsym_block!{methods {} $abi { $($body)* }} )*
        }

        impl $crate::Bindings for $name {
            #[allow(unused_mut, unused_assignments, unused_variables)]
            fn load(library: $crate::Library) -> $crate::Result<Self> {
                let mut symbols = [::core::ptr::null_mut(); $name::__DLSYM_COUNT];
                let mut index = 0usize;
                $( $crate::__dlsym_block!{load {&library, symbols, index} $abi { $($body)* }} )*
                ::core::result::Result::Ok(Self {
                    __dlsym_symbols: $crate::__private::Symbols(symbols),
                    __dlsym_library: library,
                })
            }

            fn library(&self) -> &$crate::Library {
                &self.__dlsym_library
            }
        }

        impl ::core::fmt::Debug for $name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                f.debug_struct(::core::stringify!($name))
                    .field("library", &self.__dlsym_library)
                    .finish_non_exhaustive()
            }
        }
    };
}

/// Matches the items of one `unsafe extern` block. Matching is done per
/// block (rustc's matcher is quadratic in the size of what one pattern
/// matches), and in four passes: count, names, methods and load statements.
#[doc(hidden)]
#[macro_export]
macro_rules! __dlsym_block {
    (count $abi:literal { $(
        $(#[$($attr:tt)*])* $ivis:vis $($kw:ident)+
        $(( $($args:tt)* ))? $(-> $ret:ty)? $(: $sty:ty)? ;
    )* }) => {
        0 $(+ <[&str]>::len(&[::core::stringify!($($kw)+)]))*
    };
    (names {$names:ident, $i:ident} $abi:literal { $(
        $(#[$($attr:tt)*])* $ivis:vis $($kw:ident)+
        $(( $($args:tt)* ))? $(-> $ret:ty)? $(: $sty:ty)? ;
    )* }) => {
        $(
            $names[$i] = $crate::__dlsym_name!($($kw)+);
            $i += 1;
        )*
    };
    (methods $ctx:tt $abi:literal { $(
        $(#[$($attr:tt)*])* $ivis:vis $($kw:ident)+
        $(( $($args:tt)* ))? $(-> $ret:ty)? $(: $sty:ty)? ;
    )* }) => {
        $(
            $crate::__dlsym_attrs!{(method {} $abi [$ivis] [$($kw)+]
                [$(($($args)*))?] [$(-> $ret)?] [$(: $sty)?]) [no] [] [] $(#[$($attr)*])*}
        )*
    };
    (load $ctx:tt $abi:literal { $(
        $(#[$($attr:tt)*])* $ivis:vis $($kw:ident)+
        $(( $($args:tt)* ))? $(-> $ret:ty)? $(: $sty:ty)? ;
    )* }) => {
        $(
            $crate::__dlsym_attrs!{(load $ctx $abi [$ivis] [$($kw)+]
                [$(($($args)*))?] [$(-> $ret)?] [$(: $sty)?]) [no] [] [] $(#[$($attr)*])*}
        )*
    };
}

// Per-item expansion for `library!`, split into one small macro per stage
// so that each invocation only tries a few rules. None of these are public
// API.
//
// 1. `__dlsym_attrs` sorts an item's attributes into `#[optional]`,
//    `#[link_name]` and the rest (recursing once per attribute).
// 2. `__dlsym_decode` turns its keywords into `safety kind name`.
// 3. `__dlsym_gen` emits the requested piece: a `load` statement or a
//    `method`.

/// The name of a `library!` item: the last of its keywords.
#[doc(hidden)]
#[macro_export]
macro_rules! __dlsym_name {
    (fn $n:ident) => {
        ::core::stringify!($n)
    };
    (safe fn $n:ident) => {
        ::core::stringify!($n)
    };
    (unsafe fn $n:ident) => {
        ::core::stringify!($n)
    };
    (static $n:ident) => {
        ::core::stringify!($n)
    };
    (safe static $n:ident) => {
        ::core::stringify!($n)
    };
    (unsafe static $n:ident) => {
        ::core::stringify!($n)
    };
    (static mut $n:ident) => {
        ::core::stringify!($n)
    };
    (unsafe static mut $n:ident) => {
        ::core::stringify!($n)
    };
    // Malformed items are reported by `__dlsym_decode`.
    ($($kw:ident)+) => {
        ""
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __dlsym_attrs {
    ($k:tt [$o:ident] [$($l:tt)*] [$($a:tt)*] #[optional] $($t:tt)*) => {
        $crate::__dlsym_attrs!{$k [yes] [$($l)*] [$($a)*] $($t)*}
    };
    ($k:tt [$o:ident] [$($l:tt)*] [$($a:tt)*] #[link_name = $link:literal] $($t:tt)*) => {
        $crate::__dlsym_attrs!{$k [$o] [$link] [$($a)*] $($t)*}
    };
    ($k:tt [$o:ident] [$($l:tt)*] [$($a:tt)*] #[$($x:tt)*] $($t:tt)*) => {
        $crate::__dlsym_attrs!{$k [$o] [$($l)*] [$($a)* #[$($x)*]] $($t)*}
    };
    (($mode:ident $ctx:tt $abi:literal [$vis:vis] [$($kw:ident)+] $($rest:tt)*)
        [$o:ident] [$($l:tt)*] [$($a:tt)*]) => {
        $crate::__dlsym_decode!{($mode $ctx $abi [$o] [$($l)*] [$($a)*] [$vis] $($rest)*) $($kw)+}
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __dlsym_decode {
    (($($k:tt)*) fn $n:ident) => { $crate::__dlsym_gen!{$($k)* unsafe fn $n} };
    (($($k:tt)*) unsafe fn $n:ident) => { $crate::__dlsym_gen!{$($k)* unsafe fn $n} };
    (($($k:tt)*) safe fn $n:ident) => { $crate::__dlsym_gen!{$($k)* safe fn $n} };
    (($($k:tt)*) static $n:ident) => { $crate::__dlsym_gen!{$($k)* unsafe static $n} };
    (($($k:tt)*) unsafe static $n:ident) => { $crate::__dlsym_gen!{$($k)* unsafe static $n} };
    (($($k:tt)*) safe static $n:ident) => { $crate::__dlsym_gen!{$($k)* safe static $n} };
    (($($k:tt)*) static mut $n:ident) => { $crate::__dlsym_gen!{$($k)* unsafe mut $n} };
    (($($k:tt)*) unsafe static mut $n:ident) => { $crate::__dlsym_gen!{$($k)* unsafe mut $n} };
    (($($k:tt)*) $($kw:ident)+) => {
        ::core::compile_error!(::core::concat!(
            "dlsym::library!: expected `[safe|unsafe] fn NAME(..)` or ",
            "`[safe|unsafe] static [mut] NAME: TYPE`, found `",
            ::core::stringify!($($kw)+), "`"
        ));
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __dlsym_gen {
    // ---- load: resolve the symbol's address into its slot. ----------------
    (load {$lib:expr, $syms:ident, $i:ident} $abi:literal [$o:ident] [$link:literal]
        [$($a:tt)*] [$vis:vis] $($x:tt)*) => {
        $crate::__dlsym_load_stmt!{{$syms $i ($crate::__private::resolve(
            $lib,
            const { $crate::__private::link_name(::core::concat!($link, "\0")) },
            $crate::__dlsym_optional!($o),
        )?)} [] $($a)*}
    };
    (load {$lib:expr, $syms:ident, $i:ident} $abi:literal [$o:ident] []
        [$($a:tt)*] [$vis:vis] [$($args:tt)*] [$($ret:tt)*] [$($sty:tt)*]
        $safety:ident $kind:ident $n:ident) => {
        $crate::__dlsym_load_stmt!{{$syms $i ($crate::__private::resolve(
            $lib,
            const { $crate::__private::link_name(::core::concat!(::core::stringify!($n), "\0")) },
            $crate::__dlsym_optional!($o),
        )?)} [] $($a)*}
    };

    // ---- method: the accessor. ---------------------------------------------
    // Variadic function.
    (method $ctx:tt $abi:literal [$o:ident] [$($l:tt)*] [$($a:tt)*] [$vis:vis]
        [($($arg:ident : $t:ty),+ , ...)] [$(-> $r:ty)?] [] $safety:ident fn $n:ident) => {
        $($a)*
        ///
        /// Variadic: call it through `Symbol::with`.
        #[allow(non_snake_case)]
        $vis fn $n(&self) -> $crate::__dlsym_wrap!($o
            $crate::Symbol<'_, unsafe extern $abi fn($($t),+ , ...) $(-> $r)?>)
        {
            let addr = self.__dlsym_symbols.0
                [const { $crate::__private::index_of(Self::__DLSYM_NAMES, Self::__DLSYM_ORDER, ::core::stringify!($n)) }];
            $crate::__dlsym_unwrap!($o addr => $crate::__private::symbol(self, addr))
        }
    };
    // Function.
    (method $ctx:tt $abi:literal [$o:ident] [$($l:tt)*] [$($a:tt)*] [$vis:vis]
        [($($arg:ident : $t:ty),* $(,)?)] [$(-> $r:ty)?] [] $safety:ident fn $n:ident) => {
        $crate::__dlsym_fn_method!{$safety $o [$($a)*] [$vis] $n
            [unsafe extern $abi fn($($t),*) $(-> $r)?] [$($arg : $t),*]
            [$crate::__dlsym_ret!($($r)?)]}
    };
    // Data.
    (method $ctx:tt $abi:literal [$o:ident] [$($l:tt)*] [$($a:tt)*] [$vis:vis]
        [] [] [: $t:ty] unsafe static $n:ident) => {
        $($a)*
        ///
        /// # Safety
        ///
        /// The data must not be mutated while the reference is alive.
        #[allow(non_snake_case)]
        $vis unsafe fn $n(&self) -> $crate::__dlsym_wrap!($o &$t) {
            let addr = self.__dlsym_symbols.0
                [const { $crate::__private::index_of(Self::__DLSYM_NAMES, Self::__DLSYM_ORDER, ::core::stringify!($n)) }];
            // SAFETY: `load` resolved this slot to a non-null `$t` that lives
            // as long as the library `self` owns.
            $crate::__dlsym_unwrap!($o addr => unsafe { &*addr.cast::<$t>() })
        }
    };
    (method $ctx:tt $abi:literal [$o:ident] [$($l:tt)*] [$($a:tt)*] [$vis:vis]
        [] [] [: $t:ty] safe static $n:ident) => {
        $($a)*
        #[allow(non_snake_case)]
        $vis fn $n(&self) -> $crate::__dlsym_wrap!($o &$t) {
            const { $crate::__private::assert_sync::<$t>() };
            let addr = self.__dlsym_symbols.0
                [const { $crate::__private::index_of(Self::__DLSYM_NAMES, Self::__DLSYM_ORDER, ::core::stringify!($n)) }];
            // SAFETY: as for unsafe statics, and the `unsafe extern` block
            // asserts that reading it is safe.
            $crate::__dlsym_unwrap!($o addr => unsafe { &*addr.cast::<$t>() })
        }
    };
    (method $ctx:tt $abi:literal [$o:ident] [$($l:tt)*] [$($a:tt)*] [$vis:vis]
        [] [] [: $t:ty] unsafe mut $n:ident) => {
        $($a)*
        #[allow(non_snake_case)]
        $vis fn $n(&self) -> $crate::__dlsym_wrap!($o *mut $t) {
            let addr = self.__dlsym_symbols.0
                [const { $crate::__private::index_of(Self::__DLSYM_NAMES, Self::__DLSYM_ORDER, ::core::stringify!($n)) }];
            $crate::__dlsym_unwrap!($o addr => addr.cast::<$t>())
        }
    };
    (method $ctx:tt $abi:literal [$o:ident] [$($l:tt)*] [$($a:tt)*] [$vis:vis]
        [$($args:tt)*] [$($ret:tt)*] [$($sty:tt)*] $safety:ident $kind:ident $n:ident) => {
        ::core::compile_error!(::core::concat!(
            "dlsym::library!: malformed item `", ::core::stringify!($n),
            "`: functions need `(args)` and no `: TYPE`; statics need `: TYPE`"
        ));
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __dlsym_fn_method {
    (unsafe $o:ident [$($a:tt)*] [$vis:vis] $n:ident [$f:ty] [$($arg:ident : $t:ty),*] [$r:ty]) => {
        $($a)*
        ///
        /// # Safety
        ///
        /// The call must satisfy the foreign function's contract.
        #[allow(non_snake_case, clippy::too_many_arguments)]
        $vis unsafe fn $n(&self, $($arg: $t),*) -> $crate::__dlsym_wrap!($o $r) {
            let addr = self.__dlsym_symbols.0
                [const { $crate::__private::index_of(Self::__DLSYM_NAMES, Self::__DLSYM_ORDER, ::core::stringify!($n)) }];
            $crate::__dlsym_unwrap!($o addr =>
                // SAFETY: `load` resolved this slot to a function with this
                // signature (asserted by the `unsafe extern` block), and
                // `self` owns the library. The call contract is the caller's.
                unsafe { ::core::mem::transmute::<*mut ::core::ffi::c_void, $f>(addr)($($arg),*) })
        }
    };
    (safe $o:ident [$($a:tt)*] [$vis:vis] $n:ident [$f:ty] [$($arg:ident : $t:ty),*] [$r:ty]) => {
        $($a)*
        #[allow(non_snake_case, clippy::too_many_arguments)]
        $vis fn $n(&self, $($arg: $t),*) -> $crate::__dlsym_wrap!($o $r) {
            let addr = self.__dlsym_symbols.0
                [const { $crate::__private::index_of(Self::__DLSYM_NAMES, Self::__DLSYM_ORDER, ::core::stringify!($n)) }];
            $crate::__dlsym_unwrap!($o addr =>
                // SAFETY: as for unsafe items, and the `unsafe extern` block
                // asserts that this function is safe to call.
                unsafe { ::core::mem::transmute::<*mut ::core::ffi::c_void, $f>(addr)($($arg),*) })
        }
    };
}

/// Emits `#[cfg(..)]* { symbols[index] = resolve(..)?; } index += 1;`, so
/// that items disabled by `#[cfg]` are not looked up.
#[doc(hidden)]
#[macro_export]
macro_rules! __dlsym_load_stmt {
    ($k:tt [$($c:tt)*] #[cfg $p:tt] $($t:tt)*) => {
        $crate::__dlsym_load_stmt!{$k [$($c)* $p] $($t)*}
    };
    ($k:tt [$($c:tt)*] #[$($x:tt)*] $($t:tt)*) => {
        $crate::__dlsym_load_stmt!{$k [$($c)*] $($t)*}
    };
    ({$syms:ident $i:ident ($($e:tt)*)} [$($p:tt)*]) => {
        $(#[cfg $p])*
        {
            $syms[$i] = $($e)*;
        }
        $i += 1;
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __dlsym_optional {
    (yes) => {
        true
    };
    (no) => {
        false
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __dlsym_ret {
    () => {
        ()
    };
    ($r:ty) => {
        $r
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __dlsym_wrap {
    (no $($t:tt)*) => { $($t)* };
    (yes $($t:tt)*) => { ::core::option::Option<$($t)*> };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __dlsym_unwrap {
    (no $addr:ident => $e:expr) => {
        $e
    };
    (yes $addr:ident => $e:expr) => {
        if $addr.is_null() {
            ::core::option::Option::None
        } else {
            ::core::option::Option::Some($e)
        }
    };
}
