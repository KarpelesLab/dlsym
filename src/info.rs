use std::ffi::{CString, c_void};
use std::path::PathBuf;

use crate::os::imp;

/// What the loader knows about an address: the library containing it and,
/// where available, the nearest exported symbol.
///
/// ```no_run
/// # use dlsym::{AddressInfo, Library};
/// # let lib = Library::this().unwrap();
/// let strlen = unsafe { lib.get::<*const u8>(c"strlen") }.unwrap();
/// let info = AddressInfo::of(strlen.as_ptr()).unwrap();
/// println!("strlen lives in {}", info.library.display());
/// ```
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct AddressInfo {
    /// Path of the library (or executable) containing the address.
    pub library: PathBuf,
    /// Address the library is loaded at.
    pub library_base: *mut c_void,
    /// Name of the nearest exported symbol at or below the address
    /// (Unix only; `None` on Windows or if there is none).
    pub symbol: Option<CString>,
    /// Address of that symbol (null if there is none).
    pub symbol_address: *mut c_void,
}

impl AddressInfo {
    /// Looks up `addr` (`dladdr` on Unix, `GetModuleHandleExW` +
    /// `GetModuleFileNameW` on Windows). Returns `None` if the address is not
    /// inside any loaded library.
    pub fn of(addr: *const c_void) -> Option<AddressInfo> {
        imp::address_info(addr)
    }
}

/// The path a library was loaded from, if the platform can tell.
pub(crate) fn library_path(handle: &imp::Handle) -> Option<PathBuf> {
    imp::library_path(handle)
}
