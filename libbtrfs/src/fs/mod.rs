//! Btrfs filesystem operations.
use crate::{
    bindings::{
        btrfs_ioctl_fs_info_args, btrfs_ioctl_space_args, btrfs_ioctl_space_info,
        BTRFS_FS_INFO_FLAG_CSUM_INFO, BTRFS_FS_INFO_FLAG_GENERATION,
        BTRFS_FS_INFO_FLAG_METADATA_UUID, BTRFS_IOC_FS_INFO, BTRFS_IOC_SPACE_INFO,
    },
    util::{btrfs_ioctl, IoResult},
    Flags,
};
use std::{
    alloc::{alloc, dealloc, handle_alloc_error, Layout},
    ffi::CString,
    fs::File,
    mem::{align_of, size_of, MaybeUninit},
    os::fd::{AsFd, AsRawFd},
    os::unix::prelude::OsStrExt,
    path::Path,
};
use uuid::Uuid;

pub mod block_group;

/// Information about a btrfs filesystem
///
/// This structure is returned by the [`info`] function
#[repr(transparent)]
pub struct FsInfo(btrfs_ioctl_fs_info_args);

impl FsInfo
{
    /// Maximum device id for the filesystem
    pub fn max_id(&self) -> u64
    {
        self.0.max_id
    }

    /// Number of devices associated with the filesystem
    pub fn num_devices(&self) -> u64
    {
        self.0.num_devices
    }

    /// Filesystem UUID
    pub fn fsid(&self) -> Uuid
    {
        Uuid::from_bytes(self.0.fsid)
    }

    /// Treeblock size in which btrfs stores metadata
    pub fn nodesize(&self) -> u32
    {
        self.0.nodesize
    }

    /// Minimum data block allocation unit
    pub fn sectorsize(&self) -> u32
    {
        self.0.sectorsize
    }

    pub fn clone_alignment(&self) -> u32
    {
        self.0.clone_alignment
    }

    /// Checksum type
    ///
    /// # Panics
    ///
    /// Will panic if [`Flags::CSUM_INFO`] flag was not provided
    pub fn csum_type(&self) -> u16
    {
        (self.0.flags & BTRFS_FS_INFO_FLAG_CSUM_INFO != 0)
            .then_some(self.0.csum_type)
            .expect("CSUM_INFO Flag not provided")
    }

    /// Checksum size
    ///
    /// # Panics
    ///
    /// Will panic if [`Flags::CSUM_INFO`] flag was not provided
    pub fn csum_size(&self) -> u16
    {
        (self.0.flags & BTRFS_FS_INFO_FLAG_CSUM_INFO != 0)
            .then_some(self.0.csum_size)
            .expect("CSUM_INFO Flag not provided")
    }

    /// Filesystem generation
    ///
    /// # Panics
    ///
    /// Will panic if [`Flags::GENERATION`] flag was not provided
    pub fn generation(&self) -> u64
    {
        (self.0.flags & BTRFS_FS_INFO_FLAG_GENERATION != 0)
            .then_some(self.0.generation)
            .expect("GENERATION Flag not provided")
    }

    /// Filesystem metadata uuid
    ///
    /// # Panics
    ///
    /// Will panic if [`Flags::METADATA_UUID`] flag was not provided
    pub fn metadata_uuid(&self) -> Uuid
    {
        (self.0.flags & BTRFS_FS_INFO_FLAG_METADATA_UUID != 0)
            .then_some(Uuid::from_bytes(self.0.metadata_uuid))
            .expect("METADATA_UUID Flag not provided")
    }
}

/// Check if a path references a btrfs filesystem
///
/// This function returns `Ok(true)` if `fs` can be determined to reference a btrfs filesystem.
pub fn is_btrfs<P: AsRef<Path>>(fs: P) -> IoResult<bool>
{
    let cstring = CString::new(fs.as_ref().as_os_str().as_bytes()).unwrap();

    let mut sfs = MaybeUninit::<libc::statfs>::uninit();
    unsafe {
        syscall!(statfs(cstring.as_ptr(), sfs.as_mut_ptr()))
            .map(|_| sfs.assume_init().f_type == libc::BTRFS_SUPER_MAGIC)
    }
}

/// Returns an [`FsInfo`] struct.
///
/// # Available flags:
///
/// [`Flags::CSUM_INFO `]
///
/// > Request information about checksum type and size
///
/// [`Flags::GENERATION `]
///
/// > Request information about filesystem generation
///
/// [`Flags::METADATA_UUID `]
///
/// > Request information about filesystem metadata UUID
///
/// # Examples
///
/// ```no_run
/// use libbtrfs::{fs, Flags};
///
/// let fs_info = fs::info("/", Flags::CSUM_INFO)?;
///
/// let csum_type = fs_info.csum_type();
///
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn info<P: AsRef<Path>>(fs: P, flags: Flags) -> IoResult<FsInfo>
{
    File::open(fs.as_ref()).and_then(|f| io::info(f, flags))
}

/// Returns a [`FsInfo`] struct that has been allocated on the heap.
///
/// This function is equivelent to [`info()`] except memeory has been allocated on heap instead of
/// the stack.
pub fn boxed_info<P: AsRef<Path>>(fs: P, flags: Flags) -> IoResult<FsInfo>
{
    File::open(fs.as_ref()).and_then(|f| io::info(f, flags))
}

/// Information about a chunk allocated to a particular block group
///
/// Returned by the [`get_spaces()`] iterator.
#[repr(transparent)]
pub struct SpaceInfo(btrfs_ioctl_space_info);

impl SpaceInfo
{
    /// Block type for this chunk
    pub fn block_type(&self) -> block_group::BlockType
    {
        self.0.flags.into()
    }

    /// Raid profile for this chunk
    pub fn raid_profile(&self) -> IoResult<block_group::RaidProfile>
    {
        self.0.flags.try_into()
    }

    /// Raw flags for this chunk
    pub fn flags(&self) -> u64
    {
        self.0.flags
    }

    /// Used bytes for this chunk
    pub fn used_bytes(&self) -> u64
    {
        self.0.used_bytes
    }

    /// Total bytes for this chunk
    pub fn total_bytes(&self) -> u64
    {
        self.0.total_bytes
    }
}

struct Iter
{
    total: usize,
    current: usize,
    argp: *const btrfs_ioctl_space_args,
    layout: Layout,
}

impl Iterator for Iter
{
    type Item = SpaceInfo;
    fn next(&mut self) -> Option<Self::Item>
    {
        if self.current == self.total {
            return None;
        }
        let off = self.current;
        self.current += 1;

        unsafe { Some(SpaceInfo(*(*self.argp).spaces.as_ptr().add(off))) }
    }

    fn size_hint(&self) -> (usize, Option<usize>)
    {
        let hint = self.total - self.current;

        (hint, Some(hint))
    }
}

impl ExactSizeIterator for Iter {}

impl Drop for Iter
{
    fn drop(&mut self)
    {
        unsafe { dealloc(self.argp.cast_mut().cast(), self.layout) }
    }
}

/// Iterator over space allocation for a block groups in a btrfs filesystem
///
/// Returns an iterator yeilding instances of [`SpaceInfo`] for a btrfs filesystem referenced by
/// `fs`
pub fn get_spaces<P: AsRef<Path>>(
    fs: P,
) -> IoResult<impl Iterator<Item = SpaceInfo> + ExactSizeIterator>
{
    File::open(fs.as_ref()).and_then(io::get_spaces)
}

/// Entry for I/O resources.
pub mod io
{
    use super::*;
    use std::mem::MaybeUninit;

    /// See [super::is_btrfs()]
    pub fn is_btrfs<R: AsFd>(resource: R) -> IoResult<bool>
    {
        let mut sfs = MaybeUninit::<libc::statfs>::uninit();
        unsafe {
            syscall!(fstatfs(resource.as_fd().as_raw_fd(), sfs.as_mut_ptr()))
                .map(|_| sfs.assume_init().f_type == libc::BTRFS_SUPER_MAGIC)
        }
    }

    /// See [super::info()]
    pub fn info<R: AsFd>(resource: R, flags: Flags) -> IoResult<FsInfo>
    {
        let mut info_args: FsInfo = unsafe { MaybeUninit::zeroed().assume_init() };

        info_args.0.flags = flags.to_raw_fs_info_flags()?;

        btrfs_ioctl(resource.as_fd(), BTRFS_IOC_FS_INFO, &mut info_args).map(|_| info_args)
    }

    /// See [super::boxed_info()]
    pub fn boxed_info<R: AsFd>(resource: R, flags: Flags) -> IoResult<Box<FsInfo>>
    {
        let mut info_args: Box<FsInfo> = unsafe { Box::new_zeroed().assume_init() };

        info_args.0.flags = flags.to_raw_fs_info_flags()?;

        btrfs_ioctl(resource.as_fd(), BTRFS_IOC_FS_INFO, info_args.as_mut()).map(|_| info_args)
    }

    /// See [super::get_spaces()]
    pub fn get_spaces<R: AsFd>(
        resource: R,
    ) -> IoResult<impl Iterator<Item = SpaceInfo> + ExactSizeIterator>
    {
        let mut args: btrfs_ioctl_space_args = unsafe { MaybeUninit::zeroed().assume_init() };

        btrfs_ioctl(resource.as_fd(), BTRFS_IOC_SPACE_INFO, &mut args)?;

        let total_spaces = args.total_spaces as usize;

        let size = (total_spaces * size_of::<btrfs_ioctl_space_info>())
            + size_of::<btrfs_ioctl_space_args>();

        let layout = Layout::from_size_align(size, align_of::<btrfs_ioctl_space_args>()).unwrap();

        unsafe {
            let argp = alloc(layout).cast::<btrfs_ioctl_space_args>();
            if argp.is_null() {
                handle_alloc_error(layout)
            }
            (*argp).space_slots = args.total_spaces;
            (*argp).total_spaces = 0;

            btrfs_ioctl(resource, BTRFS_IOC_SPACE_INFO, argp).map(|_| Iter {
                argp,
                layout,
                total: total_spaces,
                current: 0,
            })
        }
    }
}
