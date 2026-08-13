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

            /// Returns the inner pointer to this C string.
            ///
            /// The returned pointer will be valid for as long as `self` is, and points
            /// to a contiguous region of memory terminated with a 0 byte to represent
            /// the end of the string.
            ///
            /// The type of the returned pointer is
            /// [`*const c_char`][crate::ffi::c_char], and whether it's
            /// an alias for `*const i8` or `*const u8` is platform-specific.
            ///
            /// **WARNING**
            ///
            /// The returned pointer is read-only; writing to it (including passing it
            /// to C code that writes to it) causes undefined behavior.
            ///
            /// It is your responsibility to make sure that the underlying memory is not
            /// freed too early. For example, the following code will cause undefined
            /// behavior when `ptr` is used inside the `unsafe` block:
            ///
            /// **NOTE**: Documentation has been adapted from the Rust Standard Library's
            /// [`CStr::as_ptr`]
            pub const fn as_ptr(&self) -> *const c_char
            {
                self.0.as_ptr()
            }

            /// Returns the length of `self`. Like C's `strlen`, this does not include the nul terminator.
            ///
            /// > **Note**: This method is currently implemented as a constant-time
            /// > cast, but it is planned to alter its definition in the future to
            /// > perform the length calculation whenever this method is called.
            ///
            /// # Examples
            ///
            /// ```
            /// assert_eq!(c"foo".count_bytes(), 3);
            /// assert_eq!(c"".count_bytes(), 0);
            /// ```
            ///
            /// **NOTE**: Documentation has been adapted from the Rust Standard Library: [`CStr::count_bytes`]
            pub const fn count_bytes(&self) -> usize
            {
                self.0.count_bytes()
            }

            /// Returns `true` if `self.to_bytes()` has a length of 0.
            ///
            /// # Examples
            ///
            /// ```
            /// assert!(!c"foo".is_empty());
            /// assert!(c"".is_empty());
            /// ```
            ///
            /// **NOTE**: Documentation has been adapted from the Rust Standard Library: [`CStr::is_empty`]
            pub const fn is_empty(&self) -> bool
            {
                self.0.is_empty()
            }

            /// Converts this C string to a byte slice.
            ///
            /// The returned slice will **not** contain the trailing nul terminator that this C
            /// string has.
            ///
            /// > **Note**: This method is currently implemented as a constant-time
            /// > cast, but it is planned to alter its definition in the future to
            /// > perform the length calculation whenever this method is called.
            ///
            /// # Examples
            ///
            /// ```
            /// assert_eq!(c"foo".to_bytes(), b"foo");
            /// ```
            ///
            /// **NOTE**: Documentation has been adapted from the Rust Standard Library's
            /// [`CStr::to_bytes`]
            pub const fn to_bytes(&self) -> &[u8]
            {
                self.0.to_bytes()
            }

            /// Converts this C string to a byte slice containing the trailing 0 byte.
            ///
            /// This function is the equivalent of [`CStr::to_bytes`] except that it
            /// will retain the trailing nul terminator instead of chopping it off.
            ///
            /// > **Note**: This method is currently implemented as a 0-cost cast, but
            /// > it is planned to alter its definition in the future to perform the
            /// > length calculation whenever this method is called.
            ///
            /// # Examples
            ///
            /// ```
            /// assert_eq!(c"foo".to_bytes_with_nul(), b"foo\0");
            /// ```
            ///
            /// **NOTE**: Documentation has been adapted from the Rust Standard Library's
            /// [`CStr::to_bytes_with_nul`]
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
//
use std::ffi::{CString, FromVecWithNulError, IntoStringError, NulError};

/// Represents a owned path for Unix-conforming systems.
///
/// This struct wraps a [`CSring`] and also provides a zero cost conversion to a [`UnixPath`] via the
/// [`Deref`] trait.
#[repr(transparent)]
pub struct UnixPathBuf(CString);

impl UnixPathBuf
{
    /// Creates a new C-compatible string from a container of bytes.
    ///
    /// This function will consume the provided data and use the
    /// underlying bytes to construct a new string, ensuring that
    /// there is a trailing 0 byte. This trailing 0 byte will be
    /// appended by this function; the provided data should *not*
    /// contain any 0 bytes in it.
    ///
    /// # Examples
    ///
    /// ```ignore (extern-declaration)
    /// use std::ffi::CString;
    /// use std::os::raw::c_char;
    ///
    /// extern "C" { fn puts(s: *const c_char); }
    ///
    /// let to_print = CString::new("Hello!").expect("CString::new failed");
    /// unsafe {
    ///     puts(to_print.as_ptr());
    /// }
    /// ```
    ///
    /// # Errors
    ///
    /// This function will return an error if the supplied bytes contain an
    /// internal 0 byte. The [`NulError`] returned will contain the bytes as well as
    /// the position of the nul byte.
    ///
    /// *NOTE*: Documentation adapted from the Rust Standard Library: [`CString::new`]
    pub(crate) fn new<T: Into<Vec<u8>>>(t: T) -> Result<Self, NulError>
    {
        CString::new(t).map(Self)
    }

    /// Creates a C-compatible string by consuming a byte vector,
    /// without checking for interior 0 bytes.
    ///
    /// Trailing 0 byte will be appended by this function.
    ///
    /// This method is equivalent to [`CString::new`] except that no runtime
    /// assertion is made that `v` contains no 0 bytes, and it requires an
    /// actual byte vector, not anything that can be converted to one with Into.
    ///
    /// # Safety
    ///
    /// The caller must ensure `v` contains no nul bytes in its contents.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::ffi::CString;
    ///
    /// let raw = b"foo".to_vec();
    /// unsafe {
    ///     let c_string = CString::from_vec_unchecked(raw);
    /// }
    /// ```
    ///
    /// *NOTE*: Documentation adapted from the Rust Standard Library: [`CString::from_vec_unchecked`]
    pub(crate) unsafe fn from_vec_unchecked(v: Vec<u8>) -> Self
    {
        Self(CString::from_vec_unchecked(v))
    }

    /// Retakes ownership of a `CString` that was transferred to C via
    /// [`CString::into_raw`].
    ///
    /// Additionally, the length of the string will be recalculated from the pointer.
    ///
    /// # Safety
    ///
    /// This should only ever be called with a pointer that was earlier
    /// obtained by calling [`CString::into_raw`], and the memory it points to must not be accessed
    /// through any other pointer during the lifetime of reconstructed `CString`.
    /// Other usage (e.g., trying to take ownership of a string that was allocated by foreign code)
    /// is likely to lead to undefined behavior or allocator corruption.
    ///
    /// This function does not validate ownership of the raw pointer's memory.
    /// A double-free may occur if the function is called twice on the same raw pointer.
    /// Additionally, the caller must ensure the pointer is not dangling.
    ///
    /// It should be noted that the length isn't just "recomputed," but that
    /// the recomputed length must match the original length from the
    /// [`CString::into_raw`] call. This means the [`CString::into_raw`]/`from_raw`
    /// methods should not be used when passing the string to C functions that can
    /// modify the string's length.
    ///
    /// > **Note:** If you need to borrow a string that was allocated by
    /// > foreign code, use [`CStr`]. If you need to take ownership of
    /// > a string that was allocated by foreign code, you will need to
    /// > make your own provisions for freeing it appropriately, likely
    /// > with the foreign code's API to do that.
    ///
    /// # Examples
    ///
    /// Creates a `CString`, pass ownership to an `extern` function (via raw pointer), then retake
    /// ownership with `from_raw`:
    ///
    /// ```ignore (extern-declaration)
    /// use std::ffi::CString;
    /// use std::os::raw::c_char;
    ///
    /// extern "C" {
    ///     fn some_extern_function(s: *mut c_char);
    /// }
    ///
    /// let c_string = CString::from(c"Hello!");
    /// let raw = c_string.into_raw();
    /// unsafe {
    ///     some_extern_function(raw);
    ///     let c_string = CString::from_raw(raw);
    /// }
    /// ```
    ///
    /// *NOTE*: Documentation adapted from the Rust Standard Library: [`CString::from_raw`]
    pub(crate) unsafe fn from_raw(ptr: *mut c_char) -> Self
    {
        Self(CString::from_raw(ptr))
    }

    /// Consumes the `CString` and transfers ownership of the string to a C caller.
    ///
    /// The pointer which this function returns must be returned to Rust and reconstituted using
    /// [`CString::from_raw`] to be properly deallocated. Specifically, one
    /// should *not* use the standard C `free()` function to deallocate
    /// this string.
    ///
    /// Failure to call [`CString::from_raw`] will lead to a memory leak.
    ///
    /// The C side must **not** modify the length of the string (by writing a
    /// nul byte somewhere inside the string or removing the final one) before
    /// it makes it back into Rust using [`CString::from_raw`]. See the safety section
    /// in [`CString::from_raw`].
    ///
    /// # Examples
    ///
    /// ```
    /// use std::ffi::CString;
    ///
    /// let c_string = CString::from(c"foo");
    ///
    /// let ptr = c_string.into_raw();
    ///
    /// unsafe {
    ///     assert_eq!(b'f', *ptr as u8);
    ///     assert_eq!(b'o', *ptr.add(1) as u8);
    ///     assert_eq!(b'o', *ptr.add(2) as u8);
    ///     assert_eq!(b'\0', *ptr.add(3) as u8);
    ///
    ///     // retake pointer to free memory
    ///     let _ = CString::from_raw(ptr);
    /// }
    /// ```
    ///
    /// *NOTE*: Documentation adapted from the Rust Standard Library: [`CString::into_raw`]
    pub fn into_raw(self) -> *const c_char
    {
        self.0.into_raw()
    }

    /// Converts the `CString` into a [`String`] if it contains valid UTF-8 data.
    ///
    /// On failure, ownership of the original `CString` is returned.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::ffi::CString;
    ///
    /// let valid_utf8 = vec![b'f', b'o', b'o'];
    /// let cstring = CString::new(valid_utf8).expect("CString::new failed");
    /// assert_eq!(
    ///     cstring.into_string().expect("into_string() call failed"),
    ///     "foo"
    /// );
    ///
    /// let invalid_utf8 = vec![b'f', 0xff, b'o', b'o'];
    /// let cstring = CString::new(invalid_utf8).expect("CString::new failed");
    /// let err = cstring
    ///     .into_string()
    ///     .err()
    ///     .expect("into_string().err() failed");
    /// assert_eq!(err.utf8_error().valid_up_to(), 1);
    /// ```
    ///
    /// *NOTE*: Documentation adapted from the Rust Standard Library: [`CString::into_string`]
    pub fn into_string(self) -> Result<String, IntoStringError>
    {
        self.0.into_string()
    }

    /// Consumes the `CString` and returns the underlying byte buffer.
    ///
    /// The returned buffer does **not** contain the trailing nul
    /// terminator, and it is guaranteed to not have any interior nul
    /// bytes.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::ffi::CString;
    ///
    /// let c_string = CString::from(c"foo");
    /// let bytes = c_string.into_bytes();
    /// assert_eq!(bytes, vec![b'f', b'o', b'o']);
    /// ```
    ///
    /// *NOTE*: Documentation adapted from the Rust Standard Library: [`CString::into_bytes`]
    pub fn into_bytes(self) -> Vec<u8>
    {
        self.0.into_bytes()
    }

    /// Equivalent to [`CString::into_bytes()`] except that the
    /// returned vector includes the trailing nul terminator.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::ffi::CString;
    ///
    /// let c_string = CString::from(c"foo");
    /// let bytes = c_string.into_bytes_with_nul();
    /// assert_eq!(bytes, vec![b'f', b'o', b'o', b'\0']);
    /// ```
    /// *NOTE*: Documentation adapted from the Rust Standard Library: [`CString::into_bytes_with_nul`]
    pub fn into_bytes_with_nul(self) -> Vec<u8>
    {
        self.0.into_bytes_with_nul()
    }

    /// Returns the contents of this `CString` as a slice of bytes.
    ///
    /// The returned slice does **not** contain the trailing nul
    /// terminator, and it is guaranteed to not have any interior nul
    /// bytes. If you need the nul terminator, use
    /// [`CString::as_bytes_with_nul`] instead.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::ffi::CString;
    ///
    /// let c_string = CString::from(c"foo");
    /// let bytes = c_string.as_bytes();
    /// assert_eq!(bytes, &[b'f', b'o', b'o']);
    /// ```
    /// *NOTE*: Documentation adapted from the Rust Standard Library: [`CString::as_bytes`]
    pub fn as_bytes(&self) -> &[u8]
    {
        self.0.as_bytes()
    }

    /// Equivalent to [`CString::as_bytes()`] except that the
    /// returned slice includes the trailing nul terminator.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::ffi::CString;
    ///
    /// let c_string = CString::from(c"foo");
    /// let bytes = c_string.as_bytes_with_nul();
    /// assert_eq!(bytes, &[b'f', b'o', b'o', b'\0']);
    /// ```
    /// *NOTE*: Documentation adapted from the Rust Standard Library: [`CString::as_bytes_with_nul`]
    pub fn as_bytes_with_nul(&self) -> &[u8]
    {
        self.0.as_bytes_with_nul()
    }

    /// Extracts a [`CStr`] slice containing the entire string.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::ffi::{CStr, CString};
    ///
    /// let c_string = CString::from(c"foo");
    /// let cstr = c_string.as_c_str();
    /// assert_eq!(
    ///     cstr,
    ///     CStr::from_bytes_with_nul(b"foo\0").expect("CStr::from_bytes_with_nul failed")
    /// );
    /// ```
    ///
    /// *NOTE*: Documentation adapted from the Rust Standard Library: [`CString::as_c_str`]
    pub fn as_c_str(&self) -> &CStr
    {
        self.0.as_c_str()
    }

    /// Converts this `CString` into a boxed [`CStr`].
    ///
    /// # Examples
    ///
    /// ```
    /// let c_string = c"foo".to_owned();
    /// let boxed = c_string.into_boxed_c_str();
    /// assert_eq!(boxed.to_bytes_with_nul(), b"foo\0");
    /// ```
    ///
    /// *NOTE*: Documentation adapted from the Rust Standard Library: [`CString::into_boxed_c_str`]
    pub fn into_boxed_cstr(self) -> Box<CStr>
    {
        self.0.into_boxed_c_str()
    }

    /// Converts a <code>[Vec]<[u8]></code> to a [`CString`] without checking the
    /// invariants on the given [`Vec`].
    ///
    /// # Safety
    ///
    /// The given [`Vec`] **must** have one nul byte as its last element.
    /// This means it cannot be empty nor have any other nul byte anywhere else.
    ///
    /// # Example
    ///
    /// ```
    /// use std::ffi::CString;
    /// assert_eq!(
    ///     unsafe { CString::from_vec_with_nul_unchecked(b"abc\0".to_vec()) },
    ///     unsafe { CString::from_vec_unchecked(b"abc".to_vec()) }
    /// );
    /// ```
    ///
    /// *NOTE*: Documentation adapted from the Rust Standard Library: [`CString::from_vec_with_nul_unchecked`]
    pub(crate) unsafe fn from_vec_with_nul_unchecked(v: Vec<u8>) -> Self
    {
        Self(CString::from_vec_with_nul_unchecked(v))
    }

    /// Attempts to convert a <code>[Vec]<[u8]></code> to a [`CString`].
    ///
    /// Runtime checks are present to ensure there is only one nul byte in the
    /// [`Vec`], its last element.
    ///
    /// # Errors
    ///
    /// If a nul byte is present and not the last element or no nul bytes
    /// is present, an error will be returned.
    ///
    /// # Examples
    ///
    /// A successful conversion will produce the same result as [`CString::new`]
    /// when called without the ending nul byte.
    ///
    /// ```
    /// use std::ffi::CString;
    /// assert_eq!(
    ///     CString::from_vec_with_nul(b"abc\0".to_vec()).expect("CString::from_vec_with_nul failed"),
    ///     c"abc".to_owned()
    /// );
    /// ```
    ///
    /// An incorrectly formatted [`Vec`] will produce an error.
    ///
    /// ```
    /// use std::ffi::{CString, FromVecWithNulError};
    /// // Interior nul byte
    /// let _: FromVecWithNulError = CString::from_vec_with_nul(b"a\0bc".to_vec()).unwrap_err();
    /// // No nul byte
    /// let _: FromVecWithNulError = CString::from_vec_with_nul(b"abc".to_vec()).unwrap_err();
    /// ```
    ///
    /// *NOTE*: Documentation adapted from the Rust Standard Library: [`CString::from_vec_with_nul`]
    pub(crate) unsafe fn from_vec_with_nul(v: Vec<u8>) -> Result<Self, FromVecWithNulError>
    {
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
