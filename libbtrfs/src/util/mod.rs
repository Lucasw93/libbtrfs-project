use std::os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd, RawFd};

/// Used to represent strings returned by the Linux kernel.
///
/// If the string does not contain invaid UTF-8, then a this type represents a
/// `Borrowed(&str)`. If the string does contain invalid UTF-8, then this type represents a
/// `Owned(String)` with all invalid UTF-8 replaced with a [`char::REPLACEMENT_CHARACTER`].
pub type KernelStr<'a> = std::borrow::Cow<'a, str>;
pub(crate) type IoError = std::io::Error;
pub(crate) type IoResult<T> = std::result::Result<T, IoError>;

mod subvolume;
pub(crate) use subvolume::{
    open_parent_with_name, set_subvol_name, set_vol_name, subvol_info_args_from_root_item,
};

pub(crate) mod send;

pub enum OptionFd<'r>
{
    Owned(OwnedFd),
    Borrowed(BorrowedFd<'r>),
}

impl<'r> Clone for OptionFd<'r>
{
    fn clone(&self) -> Self
    {
        match self {
            Self::Borrowed(b) => Self::Borrowed(*b),
            Self::Owned(o) => {
                Self::Borrowed(unsafe { BorrowedFd::borrow_raw(o.as_fd().as_raw_fd()) })
            }
        }
    }
}

impl AsFd for OptionFd<'_>
{
    fn as_fd(&self) -> BorrowedFd<'_>
    {
        match self {
            Self::Owned(f) => f.as_fd(),
            Self::Borrowed(f) => f.as_fd(),
        }
    }
}

impl AsRawFd for OptionFd<'_>
{
    fn as_raw_fd(&self) -> RawFd
    {
        match self {
            Self::Owned(f) => f.as_raw_fd(),
            Self::Borrowed(f) => f.as_raw_fd(),
        }
    }
}

/// Ioctl wrapper function for the btrfs filesystem
///
/// Returns [`io::ErrorKind::Unsupported`] if the file descriptor is not btrfs
pub(crate) fn btrfs_ioctl<T, R: AsFd>(resource: R, op: libc::Ioctl, argp: *mut T) -> IoResult<()>
{
    // NOTE: All bindings for ioctl operations are generated as u64, so the cast to libc::Ioctl is
    // important for target that use i32 (or other) for the op type (musl).
    match syscall!(unsafe { ioctl(resource.as_fd().as_raw_fd(), op as libc::Ioctl, argp) }) {
        Err(e) => {
            if e.raw_os_error() == Some(libc::ENOTTY) {
                Err(std::io::ErrorKind::Unsupported.into())
            } else {
                Err(e)
            }
        }
        Ok(_) => Ok(()),
    }
}
