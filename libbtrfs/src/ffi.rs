//! FFI utilities for Unix-conforming systems.
use std::{
    ffi::{CStr, OsStr, c_char},
    fmt::{self, Debug},
    mem::transmute,
    ops::Deref,
    os::unix::ffi::OsStrExt,
    path::Path,
};

macro_rules! CStrWrapper {
    ($(#[doc = $doc:literal])+ pub struct $Name:ident) => {

        $(#[doc = $doc])+
        #[repr(transparent)]
        pub struct $Name(CStr);

        impl AsRef<[u8]> for $Name
        {
            fn as_ref(&self) -> &[u8]
            {
                self.0.to_bytes()
            }
        }

        impl Debug for $Name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                Debug::fmt(&self.0, f)
            }
        }

        impl $Name
        {
            pub(crate) const unsafe fn from_ptr<'a>(ptr: *const c_char) -> &'a Self
            {
                transmute::<&'a CStr, &'a Self>(CStr::from_ptr(ptr))
            }

            /// **For documentation, see: [`std::ffi::CStr::as_ptr`]**.
            pub const fn as_ptr(&self) -> *const c_char
            {
                self.0.as_ptr()
            }

            /// **For documentation, see: [`std::ffi::CStr::count_bytes`]**.
            pub const fn count_bytes(&self) -> usize
            {
                self.0.count_bytes()
            }

            /// **For documentation, see: [`std::ffi::CStr::is_empty`]**.
            pub const fn is_empty(&self) -> bool
            {
                self.0.is_empty()
            }

            /// **For documentation, see: [`std::ffi::CStr::to_bytes`]**.
            pub const fn to_bytes(&self) -> &[u8]
            {
                self.0.to_bytes()
            }

            /// **For documentation, see: [`std::ffi::CStr::to_bytes_with_nul`]**.
            pub const fn to_bytes_with_nul(&self) -> &[u8]
            {
                self.0.to_bytes_with_nul()
            }
        }
    }
}

CStrWrapper! {
    /// Represents a path for Unix-conforming systems.
    ///
    /// This struct wraps a [`CStr`] and also provides a zero cost conversion to a [`Path`] via the
    /// [`Deref`] trait.
    pub struct UnixPath
}

impl AsRef<OsStr> for UnixPath
{
    fn as_ref(&self) -> &OsStr
    {
        self.as_os_str()
    }
}

impl AsRef<Path> for UnixPath
{
    fn as_ref(&self) -> &Path
    {
        self
    }
}

impl Deref for UnixPath
{
    type Target = Path;

    fn deref(&self) -> &Self::Target
    {
        Path::new(OsStr::from_bytes(self.0.to_bytes()))
    }
}

CStrWrapper! {
    /// Represents a name for Unix-conforming systems.
    ///
    /// This struct wraps a [`CStr`] and also provides a zero cost conversion to a [`OsStr`] via the
    /// [`Deref`] trait.
    pub struct UnixStr
}

impl AsRef<Path> for UnixStr
{
    fn as_ref(&self) -> &Path
    {
        Path::new(self)
    }
}

impl AsRef<OsStr> for UnixStr
{
    fn as_ref(&self) -> &OsStr
    {
        self
    }
}

impl Deref for UnixStr
{
    type Target = OsStr;

    fn deref(&self) -> &Self::Target
    {
        OsStr::from_bytes(self.0.to_bytes())
    }
}

// ===========================================================================
// CString wrappers
// ===========================================================================

use std::ffi::{CString, FromVecWithNulError, IntoStringError, NulError};

/// Represents a owned path for Unix-conforming systems.
///
/// This struct wraps a [`CString`] and also provides a zero cost conversion to a [`UnixPath`] via the
/// [`Deref`] trait.
#[repr(transparent)]
pub struct UnixPathBuf(CString);

impl UnixPathBuf
{
    pub(crate) fn new<T: Into<Vec<u8>>>(t: T) -> Result<Self, NulError>
    {
        #![allow(unused)]
        CString::new(t).map(Self)
    }

    pub(crate) unsafe fn from_vec_unchecked(v: Vec<u8>) -> Self
    {
        #![allow(unused)]
        Self(CString::from_vec_unchecked(v))
    }

    pub(crate) unsafe fn from_raw(ptr: *mut c_char) -> Self
    {
        #![allow(unused)]
        Self(CString::from_raw(ptr))
    }

    /// **For documentation, see: [`std::ffi::CString::into_raw`]**.
    pub fn into_raw(self) -> *const c_char
    {
        self.0.into_raw()
    }

    /// **For documentation, see: [`std::ffi::CString::into_string`]**.
    pub fn into_string(self) -> Result<String, IntoStringError>
    {
        self.0.into_string()
    }

    /// **For documentation, see: [`std::ffi::CString::into_bytes`]**.
    pub fn into_bytes(self) -> Vec<u8>
    {
        self.0.into_bytes()
    }

    /// **For documentation, see: [`std::ffi::CString::into_bytes_with_nul`]**.
    pub fn into_bytes_with_nul(self) -> Vec<u8>
    {
        self.0.into_bytes_with_nul()
    }

    /// **For documentation, see: [`std::ffi::CString::as_bytes`]**.
    pub fn as_bytes(&self) -> &[u8]
    {
        self.0.as_bytes()
    }

    /// **For documentation, see: [`std::ffi::CString::as_bytes_with_nul`]**.
    pub fn as_bytes_with_nul(&self) -> &[u8]
    {
        self.0.as_bytes_with_nul()
    }

    /// **For documentation, see: [`std::ffi::CString::as_c_str`]**.
    pub fn as_c_str(&self) -> &CStr
    {
        self.0.as_c_str()
    }

    /// **For documentation, see: [`std::ffi::CString::into_boxed_c_str`]**.
    pub fn into_boxed_c_str(self) -> Box<CStr>
    {
        self.0.into_boxed_c_str()
    }

    /// **For documentation, see: [`std::ffi::CString::from_vec_with_nul_unchecked`]**.
    pub(crate) unsafe fn from_vec_with_nul_unchecked(v: Vec<u8>) -> Self
    {
        Self(CString::from_vec_with_nul_unchecked(v))
    }

    /// **For documentation, see: [`std::ffi::CString::from_vec_with_nul`]**.
    pub(crate) unsafe fn from_vec_with_nul(v: Vec<u8>) -> Result<Self, FromVecWithNulError>
    {
        #![allow(unused)]
        CString::from_vec_with_nul(v).map(Self)
    }
}

impl AsRef<[u8]> for UnixPathBuf
{
    fn as_ref(&self) -> &[u8]
    {
        self.0.as_bytes()
    }
}

impl AsRef<UnixPath> for UnixPathBuf
{
    fn as_ref(&self) -> &UnixPath
    {
        self
    }
}

impl AsRef<UnixStr> for UnixPathBuf
{
    fn as_ref(&self) -> &UnixStr
    {
        unsafe { UnixStr::from_ptr(self.0.as_ptr()) }
    }
}

impl Debug for UnixPathBuf
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result
    {
        Debug::fmt(&self.0, f)
    }
}

impl Deref for UnixPathBuf
{
    type Target = UnixPath;

    fn deref(&self) -> &Self::Target
    {
        unsafe { UnixPath::from_ptr(self.0.as_ptr()) }
    }
}
