//! Windows-specific functionality (`LoadLibraryExW` and friends).
//!
//! **Status:** written against `windows-sys` 0.61 but not yet compiled or
//! tested on a Windows target; see `ROADMAP.md`.

use std::ffi::{CStr, OsStr, c_void};
use std::os::windows::ffi::OsStrExt;
use std::ptr::NonNull;

use windows_sys::Win32::Foundation::{FreeLibrary, GetLastError};
use windows_sys::Win32::System::Diagnostics::Debug::{
    FORMAT_MESSAGE_FROM_SYSTEM, FORMAT_MESSAGE_IGNORE_INSERTS, FormatMessageW,
};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryExW};

use super::Lookup;
use crate::{Error, Result};

/// An owned `HMODULE`.
#[derive(Debug)]
pub(crate) struct Handle(NonNull<c_void>);

// SAFETY: module handles are process-global and the loader is thread-safe.
unsafe impl Send for Handle {}
// SAFETY: see above.
unsafe impl Sync for Handle {}

/// Formats the calling thread's last error code.
fn last_error() -> String {
    // SAFETY: `GetLastError` has no preconditions.
    let code = unsafe { GetLastError() };
    let mut buf = [0u16; 512];
    // SAFETY: `buf` is writable for `buf.len()` UTF-16 units.
    let len = unsafe {
        FormatMessageW(
            FORMAT_MESSAGE_FROM_SYSTEM | FORMAT_MESSAGE_IGNORE_INSERTS,
            std::ptr::null(),
            code,
            0,
            buf.as_mut_ptr(),
            buf.len() as u32,
            std::ptr::null(),
        )
    };
    if len == 0 {
        return format!("error code {code}");
    }
    String::from_utf16_lossy(&buf[..len as usize])
        .trim_end()
        .to_owned()
}

pub(crate) unsafe fn open(path: &OsStr) -> Result<Handle> {
    let wide: Vec<u16> = path.encode_wide().chain(Some(0)).collect();
    if let Some(position) = wide[..wide.len() - 1].iter().position(|&c| c == 0) {
        return Err(Error::InteriorNul { position });
    }
    // SAFETY: `wide` is NUL-terminated; running `DllMain` is the caller's
    // responsibility.
    let handle = unsafe { LoadLibraryExW(wide.as_ptr(), std::ptr::null_mut(), 0) };
    NonNull::new(handle).map(Handle).ok_or_else(|| Error::Open {
        path: path.to_owned(),
        message: last_error(),
    })
}

pub(crate) fn lookup(handle: &Handle, name: &CStr) -> Lookup {
    // SAFETY: the handle is live and `name` is a valid C string.
    match unsafe { GetProcAddress(handle.0.as_ptr(), name.as_ptr().cast()) } {
        Some(f) => Lookup::Found(f as *mut c_void),
        None => Lookup::NotFound(last_error()),
    }
}

pub(crate) fn close(handle: Handle) -> Result<()> {
    // SAFETY: the handle is live and ownership is consumed here, so it is
    // freed exactly once.
    if unsafe { FreeLibrary(handle.0.as_ptr()) } != 0 {
        Ok(())
    } else {
        Err(Error::Close {
            message: last_error(),
        })
    }
}
