use std::os::fd::{AsFd, AsRawFd};

pub(crate) type IoError = std::io::Error;
pub(crate) type IoResult<T> = std::result::Result<T, IoError>;

mod subvolume;
pub(crate) use subvolume::*;

#[cfg(feature = "send-stream")]
pub(crate) mod send;

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
