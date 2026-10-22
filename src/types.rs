use std::ffi::c_void;

mod sealed {
    pub trait Sealed {}
}

/// Types a symbol address can be interpreted as.
///
/// This trait is sealed. It is implemented for:
///
/// - `unsafe extern "C" fn(..) -> R` (and the `"system"`, `"C-unwind"` and
///   `"system-unwind"` ABIs) with up to 12 arguments. Null addresses are
///   rejected with [`Error::NullSymbol`](crate::Error::NullSymbol).
/// - `*const T` and `*mut T`, for data symbols. Null addresses are allowed.
///
/// Restricting the set of types prevents mistakes such as `get::<u64>(..)`,
/// which would silently reinterpret an address as an integer, or requesting a
/// safe `extern "C" fn`, which would let a foreign function be called without
/// an `unsafe` block.
///
/// # Safety
///
/// Implementors must be pointer-sized and valid for any non-null address.
pub unsafe trait SymbolType: Copy + sealed::Sealed {
    #[doc(hidden)]
    fn from_address(addr: *mut c_void) -> Option<Self>;
    #[doc(hidden)]
    fn to_address(self) -> *mut c_void;
}

/// Function pointer types that can be invoked through
/// [`Symbol::call`](crate::Symbol::call).
///
/// Arguments are passed as a tuple: `sym.call((a, b))`, `sym.call((a,))`, or
/// `sym.call(())`.
pub trait FnPtr: SymbolType {
    /// The argument list, as a tuple.
    type Args;
    /// The return type.
    type Output;

    /// Calls the function.
    ///
    /// # Safety
    ///
    /// The pointer must be valid and the call must satisfy the foreign
    /// function's contract.
    unsafe fn call(self, args: Self::Args) -> Self::Output;
}

// SAFETY: thin data pointers are pointer-sized and may hold any address.
unsafe impl<T> SymbolType for *const T {
    fn from_address(addr: *mut c_void) -> Option<Self> {
        Some(addr.cast_const().cast())
    }
    fn to_address(self) -> *mut c_void {
        self.cast_mut().cast()
    }
}
impl<T> sealed::Sealed for *const T {}

// SAFETY: see above.
unsafe impl<T> SymbolType for *mut T {
    fn from_address(addr: *mut c_void) -> Option<Self> {
        Some(addr.cast())
    }
    fn to_address(self) -> *mut c_void {
        self.cast()
    }
}
impl<T> sealed::Sealed for *mut T {}

macro_rules! impl_fn {
    ($abi:literal; $($ty:ident $val:ident),*) => {
        impl<R, $($ty),*> sealed::Sealed for unsafe extern $abi fn($($ty),*) -> R {}

        // SAFETY: function pointers are pointer-sized, and `from_address`
        // rejects null, the one value they cannot hold.
        unsafe impl<R, $($ty),*> SymbolType for unsafe extern $abi fn($($ty),*) -> R {
            // The address is reinterpreted, never dereferenced.
            #[allow(clippy::not_unsafe_ptr_arg_deref)]
            fn from_address(addr: *mut c_void) -> Option<Self> {
                if addr.is_null() {
                    return None;
                }
                // SAFETY: function pointers are pointer-sized on every
                // supported platform, and the target is non-null. Creating
                // an `unsafe fn` pointer is sound; calling it is not.
                Some(unsafe { std::mem::transmute::<*mut c_void, Self>(addr) })
            }
            fn to_address(self) -> *mut c_void {
                self as *mut c_void
            }
        }

        impl<R, $($ty),*> FnPtr for unsafe extern $abi fn($($ty),*) -> R {
            type Args = ($($ty,)*);
            type Output = R;

            unsafe fn call(self, ($($val,)*): Self::Args) -> R {
                // SAFETY: forwarded to the caller.
                unsafe { self($($val),*) }
            }
        }
    };
}

macro_rules! impl_fn_all_abis {
    ($($ty:ident $val:ident),*) => {
        impl_fn!("C"; $($ty $val),*);
        impl_fn!("C-unwind"; $($ty $val),*);
        impl_fn!("system"; $($ty $val),*);
        impl_fn!("system-unwind"; $($ty $val),*);
    };
}

impl_fn_all_abis!();
impl_fn_all_abis!(A a);
impl_fn_all_abis!(A a, B b);
impl_fn_all_abis!(A a, B b, C c);
impl_fn_all_abis!(A a, B b, C c, D d);
impl_fn_all_abis!(A a, B b, C c, D d, E e);
impl_fn_all_abis!(A a, B b, C c, D d, E e, F f);
impl_fn_all_abis!(A a, B b, C c, D d, E e, F f, G g);
impl_fn_all_abis!(A a, B b, C c, D d, E e, F f, G g, H h);
impl_fn_all_abis!(A a, B b, C c, D d, E e, F f, G g, H h, I i);
impl_fn_all_abis!(A a, B b, C c, D d, E e, F f, G g, H h, I i, J j);
impl_fn_all_abis!(A a, B b, C c, D d, E e, F f, G g, H h, I i, J j, K k);
impl_fn_all_abis!(A a, B b, C c, D d, E e, F f, G g, H h, I i, J j, K k, L l);
