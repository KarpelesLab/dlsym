use std::borrow::Cow;
use std::ffi::{CStr, CString};

use crate::{Error, Result};

/// Types that can name a symbol.
///
/// Implemented for [`CStr`] (zero-cost; use `c"name"` literals) and for string
/// and byte slices, which are copied into a temporary NUL-terminated buffer
/// unless they already end with a NUL byte.
pub trait SymbolName {
    /// Returns the name as a C string.
    fn to_symbol_name(&self) -> Result<Cow<'_, CStr>>;
}

impl SymbolName for CStr {
    fn to_symbol_name(&self) -> Result<Cow<'_, CStr>> {
        Ok(Cow::Borrowed(self))
    }
}

impl SymbolName for CString {
    fn to_symbol_name(&self) -> Result<Cow<'_, CStr>> {
        Ok(Cow::Borrowed(self.as_c_str()))
    }
}

impl SymbolName for [u8] {
    fn to_symbol_name(&self) -> Result<Cow<'_, CStr>> {
        match CStr::from_bytes_until_nul(self) {
            Ok(s) if s.count_bytes() + 1 == self.len() => Ok(Cow::Borrowed(s)),
            Ok(s) => Err(Error::InteriorNul {
                position: s.count_bytes(),
            }),
            Err(_) => CString::new(self)
                .map(Cow::Owned)
                .map_err(|e| Error::InteriorNul {
                    position: e.nul_position(),
                }),
        }
    }
}

impl SymbolName for str {
    fn to_symbol_name(&self) -> Result<Cow<'_, CStr>> {
        self.as_bytes().to_symbol_name()
    }
}

impl SymbolName for String {
    fn to_symbol_name(&self) -> Result<Cow<'_, CStr>> {
        self.as_str().to_symbol_name()
    }
}

impl<T: SymbolName + ?Sized> SymbolName for &T {
    fn to_symbol_name(&self) -> Result<Cow<'_, CStr>> {
        (**self).to_symbol_name()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn borrows_when_possible() {
        assert!(matches!(c"cos".to_symbol_name(), Ok(Cow::Borrowed(_))));
        assert!(matches!("cos\0".to_symbol_name(), Ok(Cow::Borrowed(_))));
        assert!(matches!("cos".to_symbol_name(), Ok(Cow::Owned(_))));
    }

    #[test]
    fn rejects_interior_nul() {
        assert!(matches!(
            "co\0s".to_symbol_name(),
            Err(Error::InteriorNul { position: 2 })
        ));
        assert!(matches!(
            "co\0s\0".to_symbol_name(),
            Err(Error::InteriorNul { position: 2 })
        ));
    }
}
