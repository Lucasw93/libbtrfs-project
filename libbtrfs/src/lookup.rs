//! Btrfs filesystem lookups
use crate::{
    bindings::{
        BTRFS_FIRST_FREE_OBJECTID, BTRFS_IOC_INO_LOOKUP, BTRFS_IOC_INO_LOOKUP_USER,
        btrfs_ioctl_ino_lookup_args, btrfs_ioctl_ino_lookup_user_args,
    },
    ffi::*,
    util::{IoError, IoResult, btrfs_ioctl},
};
#[allow(unused_imports)]
use std::io::ErrorKind;
use std::os::fd::BorrowedFd;
use std::{fs::File, mem::MaybeUninit, os::fd::AsFd, path::Path, ptr};

/// Userspace lookup buffer
pub struct UserLookup<R: AsFd>(MaybeUninit<btrfs_ioctl_ino_lookup_user_args>, R);

impl<'r> From<BorrowedFd<'r>> for UserLookup<BorrowedFd<'r>>
{
    /// Initialise a userspace lookup buffer from a raw file descriptor.
    /// The file descriptor will not be closed when `Lookup` instance is dropped
    #[inline(always)]
    fn from(value: BorrowedFd<'r>) -> Self
    {
        Self(MaybeUninit::uninit(), value)
    }
}

impl TryFrom<&str> for UserLookup<File>
{
    type Error = IoError;

    #[inline(always)]
    fn try_from(value: &str) -> Result<Self, Self::Error>
    {
        Self::try_from(Path::new(value))
    }
}

impl TryFrom<&Path> for UserLookup<File>
{
    type Error = IoError;

    #[inline(always)]
    fn try_from(value: &Path) -> Result<Self, Self::Error>
    {
        File::open(value).map(|f| Self(MaybeUninit::uninit(), f))
    }
}

impl<R: AsFd> UserLookup<R>
{
    /// Lookup the path from a subvolume root
    ///
    /// Returns a lookup path and name for the subvolume referenced by `treeid`. `dirid` is the inode
    /// of the directory in which the subvolume is rooted. The path is relative to the path or file
    /// descriptor used to construct the `UserLookup` instance.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::NotFound`]
    ///
    /// > The subvolume referenced by the underlying path or file descriptor used to construct the
    /// `UserLookup` instance, does not contain the inode given by `dirid` or a subvolumeID given
    /// by `treeid`.
    ///
    /// [`ErrorKind::InvalidInput`]
    ///
    /// > `dirid` is not the directory in which the subvolume, `treeid` is rooted.
    pub fn path_str(&mut self, dirid: u64, treeid: u64) -> IoResult<(&UnixPath, &UnixStr)>
    {
        unsafe {
            let args = self.0.as_mut_ptr();
            let path = &raw mut (*args).path;
            let name = &raw mut (*args).name;

            ptr::write(&raw mut (*args).dirid, dirid);
            ptr::write(&raw mut (*args).treeid, treeid);
            ptr::write_bytes(name, 0, 1);
            ptr::write_bytes(path, 0, 1);
            btrfs_ioctl(self.1.as_fd(), BTRFS_IOC_INO_LOOKUP_USER, args).map(|_| {
                (
                    UnixPath::from_ptr(path.cast()),
                    UnixStr::from_ptr(name.cast()),
                )
            })
        }
    }
}

/// Lookup buffer
pub struct Lookup<R: AsFd>(MaybeUninit<btrfs_ioctl_ino_lookup_args>, R);

impl<'r> From<BorrowedFd<'r>> for Lookup<BorrowedFd<'r>>
{
    /// Initialise a lookup buffer from a raw file descriptor. The file descriptor will
    /// not be closed when `Lookup` instance is dropped
    #[inline(always)]
    fn from(value: BorrowedFd<'r>) -> Self
    {
        Self(MaybeUninit::uninit(), value)
    }
}

impl TryFrom<&str> for Lookup<File>
{
    type Error = IoError;

    #[inline(always)]
    fn try_from(value: &str) -> Result<Self, Self::Error>
    {
        Self::try_from(Path::new(value))
    }
}

impl TryFrom<&Path> for Lookup<File>
{
    type Error = IoError;

    #[inline(always)]
    fn try_from(value: &Path) -> Result<Self, Self::Error>
    {
        File::open(value).map(|f| Self(MaybeUninit::uninit(), f))
    }
}

impl<R: AsFd> Lookup<R>
{
    /// Lookup the treeid for the subvolume referenced by the underlying path or file descriptor.
    pub fn treeid(&mut self) -> IoResult<u64>
    {
        unsafe {
            let arg_ptr = self.0.as_mut_ptr();

            ptr::write(&raw mut (*arg_ptr).objectid, BTRFS_FIRST_FREE_OBJECTID);
            ptr::write(&raw mut (*arg_ptr).treeid, 0);
            ptr::write_bytes(&raw mut (*arg_ptr).name, 0, 1);
            btrfs_ioctl(self.1.as_fd(), BTRFS_IOC_INO_LOOKUP, arg_ptr).map(|_| (*arg_ptr).treeid)
        }
    }

    /// Lookup path from an inode to a subvolume root
    ///
    /// This function retrurns a lookup path as a string slice to `objectid` which is the inode of a
    /// file or directory which must be containded in the subvolume referenced by `treeid`. The returned
    /// path is relative to the subvolume referenced by `treeid`. `objectid` and `treeid` must both be
    /// within the btrfs filesystem referenced by `fs`.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::InvalidData`]
    ///
    /// > The path to be returned contained invalid UTF-8.
    ///
    /// # Notes
    ///
    /// **Requires CAP_SYS_ADMIN capabilities**
    pub fn path_str(&mut self, objectid: u64, treeid: u64) -> IoResult<&UnixPath>
    {
        unsafe {
            let args = self.0.as_mut_ptr();
            let name = &raw mut (*args).name;

            ptr::write(&raw mut (*args).objectid, objectid);
            ptr::write(&raw mut (*args).treeid, treeid);
            ptr::write_bytes(name, 0, 1);
            btrfs_ioctl(self.1.as_fd(), BTRFS_IOC_INO_LOOKUP, args)
                .map(|_| UnixPath::from_ptr(name.cast()))
        }
    }
}
