//! Support code for [`library!`](crate::library) expansions. Not public API.

use std::ffi::{CStr, c_void};

use crate::os::{Lookup, imp};
use crate::{Error, Library, Result, Symbol, SymbolType};

/// The resolved addresses of a `library!` struct, in declaration order. Null
/// means an `#[optional]` symbol that was not found.
pub struct Symbols<const N: usize>(pub [*mut c_void; N]);

// SAFETY: the addresses are only dereferenced through the generated methods,
// which carry their own safety contracts (and `Sync` checks for safe data).
unsafe impl<const N: usize> Send for Symbols<N> {}
// SAFETY: see above.
unsafe impl<const N: usize> Sync for Symbols<N> {}

/// Resolves `name`, returning null for missing `optional` symbols.
pub fn resolve(lib: &Library, name: &CStr, optional: bool) -> Result<*mut c_void> {
    match imp::lookup(lib.handle(), name) {
        Lookup::Found(addr) if !addr.is_null() => Ok(addr),
        _ if optional => Ok(std::ptr::null_mut()),
        Lookup::Found(_) => Err(Error::NullSymbol { name: name.into() }),
        Lookup::NotFound(message) => Err(Error::SymbolNotFound {
            name: name.into(),
            message,
        }),
    }
}

/// Wraps an address resolved by `owner` into a [`Symbol`] borrowing `owner`.
pub fn symbol<O: ?Sized, F: SymbolType>(_owner: &O, addr: *mut c_void) -> Symbol<'_, F> {
    // `load` never stores null for a non-optional symbol, and optional ones
    // are checked by the caller.
    Symbol::new(F::from_address(addr).expect("dlsym: null symbol"))
}

/// Returns `(hash, index)` for each of `names`, sorted by hash. Evaluated
/// once per `library!` struct at compile time, so that [`index_of`] can
/// binary-search. Sorting 64-bit hashes rather than strings keeps the
/// compile-time cost low for thousands of names.
pub const fn sort_order<const N: usize>(names: &[&str]) -> [(u64, usize); N] {
    assert!(names.len() == N);
    let mut order = [(0u64, 0usize); N];
    let mut i = 0;
    while i < N {
        order[i] = (fnv1a(names[i]), i);
        i += 1;
    }
    // Heapsort: in place, iterative, O(N log N).
    let mut start = N / 2;
    while start > 0 {
        start -= 1;
        sift_down(&mut order, start, N);
    }
    let mut end = N;
    while end > 1 {
        end -= 1;
        let tmp = order[0];
        order[0] = order[end];
        order[end] = tmp;
        sift_down(&mut order, 0, end);
    }
    order
}

const fn sift_down(order: &mut [(u64, usize)], mut root: usize, end: usize) {
    loop {
        let mut child = 2 * root + 1;
        if child >= end {
            return;
        }
        if child + 1 < end && order[child].0 < order[child + 1].0 {
            child += 1;
        }
        if order[root].0 >= order[child].0 {
            return;
        }
        let tmp = order[root];
        order[root] = order[child];
        order[child] = tmp;
        root = child;
    }
}

/// Returns the position of `name` in `names`, given `order` from
/// [`sort_order`]. Evaluated at compile time.
pub const fn index_of(names: &[&str], order: &[(u64, usize)], name: &str) -> usize {
    let hash = fnv1a(name);
    let (mut lo, mut hi) = (0, order.len());
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if order[mid].0 < hash {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    // `lo` is the first entry with this hash; check each (collisions).
    while lo < order.len() && order[lo].0 == hash {
        if str_cmp(names[order[lo].1], name) == 0 {
            return order[lo].1;
        }
        lo += 1;
    }
    panic!("dlsym: symbol name not found")
}

/// 64-bit FNV-1a.
const fn fnv1a(s: &str) -> u64 {
    let bytes = s.as_bytes();
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let mut i = 0;
    while i < bytes.len() {
        hash ^= bytes[i] as u64;
        hash = hash.wrapping_mul(0x0100_0000_01b3);
        i += 1;
    }
    hash
}

/// Lexicographic byte comparison, as `-1`, `0` or `1`.
const fn str_cmp(a: &str, b: &str) -> i32 {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let mut i = 0;
    while i < a.len() && i < b.len() {
        if a[i] != b[i] {
            return if a[i] < b[i] { -1 } else { 1 };
        }
        i += 1;
    }
    if a.len() < b.len() {
        -1
    } else if a.len() > b.len() {
        1
    } else {
        0
    }
}

/// Turns `"name\0"` (possibly `"r#name\0"` from a raw identifier) into the
/// symbol name to look up.
pub const fn link_name(s: &'static str) -> &'static CStr {
    let bytes = s.as_bytes();
    let bytes = if bytes.len() > 2 && bytes[0] == b'r' && bytes[1] == b'#' {
        bytes.split_at(2).1
    } else {
        bytes
    };
    match CStr::from_bytes_with_nul(bytes) {
        Ok(name) => name,
        Err(_) => panic!("dlsym: symbol name contains a NUL byte"),
    }
}

/// Fails to compile unless `T` is `Sync`.
pub const fn assert_sync<T: ?Sized + Sync>() {}

/// Reads a `width`-bit field starting `bit` bits into `storage`, as laid out
/// by the C compiler for the target's byte order.
pub fn bitfield_get(storage: &[u8], bit: u64, width: u32) -> u64 {
    let mut value = 0u64;
    for i in 0..u64::from(width) {
        let b = bit + i;
        if storage[(b / 8) as usize] & bit_mask(b) != 0 {
            value |= 1 << value_bit(i, width);
        }
    }
    value
}

/// Writes the low `width` bits of `value` into a bitfield; see
/// [`bitfield_get`].
pub fn bitfield_set(storage: &mut [u8], bit: u64, width: u32, value: u64) {
    for i in 0..u64::from(width) {
        let b = bit + i;
        let byte = &mut storage[(b / 8) as usize];
        if value & (1 << value_bit(i, width)) != 0 {
            *byte |= bit_mask(b);
        } else {
            *byte &= !bit_mask(b);
        }
    }
}

/// Little-endian ABIs allocate bitfields from the least significant bit of
/// each byte, big-endian ones from the most significant.
const fn bit_mask(b: u64) -> u8 {
    if cfg!(target_endian = "little") {
        1 << (b % 8)
    } else {
        0x80 >> (b % 8)
    }
}

/// Which bit of the value is stored `i` bits into the field.
const fn value_bit(i: u64, width: u32) -> u64 {
    if cfg!(target_endian = "little") {
        i
    } else {
        width as u64 - 1 - i
    }
}

/// Heap storage for one C++ object of type `T`, as used by generated
/// object wrappers: the constructor runs in it, and the storage is freed
/// when this is dropped (e.g. if the constructor unwinds) unless
/// [`into_raw`](Self::into_raw) took it over.
pub struct ObjectStorage<T> {
    ptr: *mut T,
}

impl<T> ObjectStorage<T> {
    /// Allocates uninitialized storage.
    ///
    /// # Panics
    ///
    /// If `T` is zero-sized (C++ objects never are).
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let layout = std::alloc::Layout::new::<T>();
        assert!(layout.size() != 0, "C++ objects are never zero-sized");
        // SAFETY: the layout has a non-zero size.
        let ptr = unsafe { std::alloc::alloc(layout) }.cast::<T>();
        if ptr.is_null() {
            std::alloc::handle_alloc_error(layout);
        }
        ObjectStorage { ptr }
    }

    /// The storage's address.
    pub fn as_ptr(&self) -> *mut T {
        self.ptr
    }

    /// Takes over the storage, to be released with [`free`](Self::free).
    pub fn into_raw(self) -> *mut T {
        let ptr = self.ptr;
        std::mem::forget(self);
        ptr
    }

    /// Releases storage returned by [`into_raw`](Self::into_raw) (whose
    /// object must already be destroyed).
    ///
    /// # Safety
    ///
    /// `ptr` must come from `into_raw` and not have been freed.
    pub unsafe fn free(ptr: *mut T) {
        // SAFETY: `ptr` was allocated by `new` with this layout.
        unsafe { std::alloc::dealloc(ptr.cast(), std::alloc::Layout::new::<T>()) };
    }
}

impl<T> Drop for ObjectStorage<T> {
    fn drop(&mut self) {
        // SAFETY: `self` still owns the allocation.
        unsafe { Self::free(self.ptr) };
    }
}
