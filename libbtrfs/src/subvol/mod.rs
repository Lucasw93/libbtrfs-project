//! Btrfs subvolume operations.
//!
//! Basic management of subvolumes in a btrfs filesystem.
use crate::{
    bindings::{
        btrfs_ioctl_get_subvol_info_args, btrfs_ioctl_timespec, btrfs_ioctl_vol_args_v2,
        BTRFS_DIR_ITEM_KEY, BTRFS_FIRST_FREE_OBJECTID, BTRFS_IOC_DEFAULT_SUBVOL,
        BTRFS_IOC_GET_SUBVOL_INFO, BTRFS_IOC_SNAP_CREATE_V2, BTRFS_IOC_SNAP_DESTROY_V2,
        BTRFS_IOC_SUBVOL_CREATE_V2, BTRFS_IOC_SUBVOL_GETFLAGS, BTRFS_IOC_SUBVOL_SETFLAGS,
        BTRFS_ROOT_TREE_DIR_OBJECTID, BTRFS_SUBVOL_RDONLY, BTRFS_SUBVOL_SPEC_BY_ID,
    },
    fs, lookup,
    tree_search::tree_item::{DirItem, RootBackref, TreeItem, TreeItemName},
    tree_search::{SearchBuilder, SearchKeyBuilder, TreeId},
    util::{btrfs_ioctl, open_parent_with_name, set_subvol_name, IoResult},
};
use std::{
    ffi::{CStr, OsString},
    fs::File,
    io::ErrorKind,
    mem::MaybeUninit,
    os::fd::AsFd,
    os::unix::{fs::MetadataExt, io::AsRawFd},
    path::Path,
};
use uuid::Uuid;

// To be removed. See note.
mod iter;
#[doc(hidden)]
pub use iter::{walk, walk_user, Iter, IterUser, SubvolEntry};

mod rootref;
pub use rootref::{get_rootref, SubvolRootRef};

mod info;
pub use info::{get_boxed_info, get_info, get_info_by_id, SubvolInfo, Timespec};

/// Btrfs subvolume snapshots
pub mod snap
{
    use super::*;
    /// Create a btrfs snapshot
    ///
    /// This function will attempt to create a btrfs snapshot named `pathname` of the subvolume
    /// referenced by `snapvol`. The `readonly` argument determines the read-only status for the
    /// snapshot. The owner and group for the The newly created snapshot will be the same as the
    /// subvolume referened by `snapvol`
    ///
    /// # Errors
    ///
    /// [`ErrorKind::AlreadyExists`]
    ///
    /// > `pathname` refers to to a file that already exists.
    ///
    /// [`ErrorKind::CrossesDevices`]
    ///
    /// > `snapvol` does not refer to a file or directory within the same filesystem as `pathname`.
    ///
    /// [`ErrorKind::InvalidInput`]
    ///
    /// > `snapvol` does not refer to a subvolume root.
    ///
    /// [`ErrorKind::NotADirectory`]
    ///
    /// > A component used as a directory in `pathname` is not, in fact, a directory.
    ///
    /// [`ErrorKind::PermissionDenied`]
    ///
    /// > Filesystem UID for the current process does not match the UID of the subvolume referenced
    /// by `snapvol`. Note that this is not required if the current user has `CAP_FOWNER`
    /// permissions.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// let snapvol = "/path/to/subvolume/named/foo";
    ///
    /// // create a read-only snapshot of `foo` called `foo_snapshot`
    /// libbtrfs::snap::create(snapvol, "/.snapshots/foo_snapshot", true)?;
    ///
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn create<P: AsRef<Path>>(snapvol: P, pathname: P, readonly: bool) -> IoResult<()>
    {
        let snapvol = File::open(snapvol)?;

        open_parent_with_name(pathname.as_ref())
            .and_then(|(dir, name)| io::create(snapvol.into(), dir, name, readonly))
    }

    /// Entry for I/O resources.
    pub mod io
    {
        use super::*;

        /// See [super::create()]
        pub fn create<R: AsFd, N: AsRef<[u8]>>(
            snapvol: R,
            dir: R,
            name: N,
            readonly: bool,
        ) -> IoResult<()>
        {
            let mut vol_args: btrfs_ioctl_vol_args_v2 =
                unsafe { MaybeUninit::zeroed().assume_init() };

            if readonly {
                vol_args.flags |= BTRFS_SUBVOL_RDONLY
            }
            vol_args.fd = snapvol.as_fd().as_raw_fd() as i64;

            set_subvol_name(name.as_ref(), unsafe { &mut vol_args.inner2.name })
                .and_then(|_| btrfs_ioctl(dir, BTRFS_IOC_SNAP_CREATE_V2, &mut vol_args))
        }
    }
}

/// Check if a path represents a btrfs subvolume
///
/// This function returns `Ok(true)` if `subvol` can be determined to represent a btrfs subvolume.
pub fn is_subvol<P: AsRef<Path>>(subvol: P) -> IoResult<bool>
{
    if !fs::is_btrfs(subvol.as_ref())? {
        return Ok(false);
    }
    subvol
        .as_ref()
        .metadata()
        .map(|m| m.is_dir() && m.ino() == BTRFS_FIRST_FREE_OBJECTID)
}

/// Create a btrfs subvolume
///
/// This function attempts to create a btrfs subvolume named `pathname`.
///
/// The newly created subvolume will be owned by the effective user ID of the calling process.
///
/// # Errors
///
/// [`ErrorKind::AlreadyExists`]
///
/// > `pathname` already exists.
///
/// [`ErrorKind::NotFound`]
///
/// > A directory component in pathname does not exist or is a dangling symbolic link.
///
/// [`ErrorKind::PermissionDenied`]
///
/// > Read/Write permissions to the parent directory is not allowed or search permissions is denied
/// for on the the directorys in path prefix of `subvol`.
pub fn create<P: AsRef<Path>>(pathname: P) -> IoResult<()>
{
    open_parent_with_name(pathname.as_ref()).and_then(|(dir, name)| io::create(dir, name))
}

/// Remove a btrfs subvolume
///
/// This function attempts to remove a btrfs subvolume referenced by `subvol`. The subvolume cannot
/// contain nested subvolumes, however it can contains regular files and directory's which will be
/// deleted if the call succeeds.
///
/// # Errors
///
/// [`ErrorKind::DirectoryNotEmpty`]
///
/// > The subvolume contained nested subvolumes.
///
/// [`ErrorKind::InvalidInput`]
///
/// > `subvol` is not a subvolume root.
///
/// [`ErrorKind::NotADirectory`]
///
/// > `subvol`, or a component used as a directory in `subvol`, is not, in fact, a directory.
///
/// [`ErrorKind::NotFound`]
///
/// > No file exists at `subvol`.
///
/// # Notes
///
/// **Requires CAP_SYS_ADMIN capabilities — (*unless the filesystem is mounted with
/// user_subvol_rm_allowed*)**
pub fn destroy<P: AsRef<Path>>(subvol: P) -> IoResult<()>
{
    open_parent_with_name(subvol.as_ref()).and_then(|(dir, name)| io::destroy(dir, name))
}

/// Remove a btrfs subvolume by its subvolume id
///
/// This function attempts to remove a btrfs subvolume by its subvolume id in a btrfs filesystem
/// referenced by `fs`. The subvolume cannot contain nested subvolumes, however it can contains
/// regular files and directory's which will be deleted if the call succeeds.
///
/// # Errors
///
/// [`ErrorKind::DirectoryNotEmpty`]
///
/// > The subvolume contained nested subvolumes.
///
/// [`ErrorKind::NotFound`]
///
/// > `subvolid` is not a valid subvolume id.
///
/// # Notes
///
/// **Requires CAP_SYS_ADMIN capabilities — (*unless the filesystem is mounted with
/// user_subvol_rm_allowed*)**
pub fn destroy_by_id<P: AsRef<Path>>(subvolid: u64, fs: P) -> IoResult<()>
{
    File::open(fs.as_ref()).and_then(|f| io::destroy_by_id(subvolid, f))
}

/// Gets the full subvolume path to the filesystem root
///
/// This function returns The full path to the filesystem root for the subvolume with id of
/// `treeid` in the btrfs filesystem referend by `fs`. This path is not relative to a btrfs mount
/// point but is relative to the level 5 (BTRFS_FS_TREE_OBJECTID) subvolume for the filesystem.
///
/// # Notes
///
/// **Requires CAP_SYS_ADMIN capabilities**
pub fn get_path<P: AsRef<Path>>(treeid: u64, fs: P) -> IoResult<OsString>
{
    File::open(fs.as_ref()).and_then(|f| io::get_path(treeid, f))
}

/// Gets the default subvolume for the filesystem
///
/// # Notes
///
/// **Requires CAP_SYS_ADMIN capabilities**
pub fn get_default<P: AsRef<Path>>(fs: P) -> IoResult<u64>
{
    File::open(fs.as_ref()).and_then(io::get_default)
}

/// Sets the default subvolume for a btrfs filesystem
///
/// # Errors
///
/// [`ErrorKind::NotFound`]
///
/// > `id` is not a valid subvolume id.
///
/// # Notes
///
/// **Requires CAP_SYS_ADMIN capabilities**
pub fn set_default<P: AsRef<Path>>(id: u64, fs: P) -> IoResult<()>
{
    File::open(fs.as_ref()).and_then(|f| io::set_default(id, f))
}

/// Gets the read-only status for a subvolume
///
/// # Errors
///
/// [`ErrorKind::InvalidInput`]
///
/// > `subvol` is not a subvolume root.
///
/// [`ErrorKind::NotFound`]
///
/// > No file exists at `subvol`.
///
/// [`ErrorKind::PermissionDenied`]
///
/// > Read access for `subvol` is not allowed, or search permission is denied for one of the
/// directorys in path prefix of `subvol`.
pub fn is_readonly<P: AsRef<Path>>(subvol: P) -> IoResult<bool>
{
    File::open(subvol.as_ref()).and_then(io::is_readonly)
}

/// Sets the readonly flag for a subvolume
///
/// <div class="warning">
///
/// Please note that setting the readonly status of the subvolume that currently contains the
/// rust project using libbtrfs may cause problems because you will no longer be able to edit the
/// rust project to change the readonly status back. Btrfs-progs does not contain a command to
/// change readonly status.
///
/// One way to solve this is to copying the project to another subvolume that is read/write then
/// set the read-only status by calling [`set_readonly`].
///
/// </div>
///
/// # Errors
///
/// [`ErrorKind::InvalidInput`]
///
/// > `subvol` is not a subvolume root.
///
/// [`ErrorKind::NotFound`]
///
/// > No file exists at `subvol`.
///
/// [`ErrorKind::PermissionDenied`]
///
/// Read access for `subvol` is not allowed, or search permission is denied for one of the
/// directorys in path prefix of `subvol`.
pub fn set_readonly<P: AsRef<Path>>(subvol: P, readonly: bool) -> IoResult<()>
{
    File::open(&subvol).and_then(|f| io::set_readonly(f, readonly))
}

/// Entry for I/O resources.
pub mod io
{
    use super::*;
    use std::mem::{ManuallyDrop, MaybeUninit};
    use std::os::{fd::FromRawFd, unix::ffi::OsStringExt};

    // To be removed. See note.
    #[doc(hidden)]
    pub use iter::io::{walk, walk_user};

    pub use super::SubvolRootRef;
    pub use rootref::io::get_rootref;

    pub use super::SubvolInfo;
    pub use info::io::{get_boxed_info, get_info, get_info_by_id};

    /// See [super::is_subvol()]
    pub fn is_subvol<R: AsFd>(fs: R) -> IoResult<bool>
    {
        if !fs::io::is_btrfs(fs.as_fd())? {
            return Ok(false);
        }
        let f = unsafe { ManuallyDrop::new(File::from_raw_fd(fs.as_fd().as_raw_fd())) };

        f.metadata()
            .map(|m| m.is_dir() && m.ino() == BTRFS_FIRST_FREE_OBJECTID)
    }

    /// See [super::create()]
    pub fn create<R: AsFd, N: AsRef<[u8]>>(dir: R, name: N) -> IoResult<()>
    {
        let mut vol_args: btrfs_ioctl_vol_args_v2 = unsafe { MaybeUninit::zeroed().assume_init() };

        set_subvol_name(name.as_ref(), unsafe { &mut vol_args.inner2.name })
            .and_then(|_| btrfs_ioctl(dir.as_fd(), BTRFS_IOC_SUBVOL_CREATE_V2, &mut vol_args))
    }

    /// See [super::destroy()]
    pub fn destroy<R: AsFd, N: AsRef<[u8]>>(dir: R, name: N) -> IoResult<()>
    {
        let mut vol_args: btrfs_ioctl_vol_args_v2 = unsafe { MaybeUninit::zeroed().assume_init() };

        set_subvol_name(name.as_ref(), unsafe { &mut vol_args.inner2.name })
            .and_then(|_| btrfs_ioctl(dir.as_fd(), BTRFS_IOC_SNAP_DESTROY_V2, &mut vol_args))
    }

    /// See [super::destroy_by_id()]
    pub fn destroy_by_id<R: AsFd>(subvolid: u64, resource: R) -> IoResult<()>
    {
        let mut vol_args: btrfs_ioctl_vol_args_v2 = unsafe { MaybeUninit::zeroed().assume_init() };

        vol_args.flags = BTRFS_SUBVOL_SPEC_BY_ID;
        vol_args.inner2.subvolid = subvolid;

        btrfs_ioctl(resource.as_fd(), BTRFS_IOC_SNAP_DESTROY_V2, &mut vol_args)
    }

    /// See [super::get_default()]
    pub fn get_default<R: AsFd>(resource: R) -> IoResult<u64>
    {
        let mut search = SearchBuilder::from(resource.as_fd())
            .tree(TreeId::RootTree)
            .item_limit(u32::MAX)
            .objectid(..=BTRFS_ROOT_TREE_DIR_OBJECTID)
            .item_type(..=BTRFS_DIR_ITEM_KEY)
            .new();

        loop {
            let items = search.search(|k| (k.0, k.1, k.2 + 1))?;

            if items.len() == 0 {
                return Err(ErrorKind::NotFound.into());
            } else if let Some(di) = items
                .filter_map(|item| item.get::<DirItem>())
                .find(|di| di.name_bytes().unwrap_or_default() == b"default")
            {
                return Ok(di.location().objectid());
            }
        }
    }

    /// See [super::set_default()]
    pub fn set_default<R: AsFd>(mut id: u64, fs: R) -> IoResult<()>
    {
        btrfs_ioctl(fs.as_fd(), BTRFS_IOC_DEFAULT_SUBVOL, &mut id)
    }

    /// See [super::is_readonly()]
    pub fn is_readonly<R: AsFd>(fs: R) -> IoResult<bool>
    {
        let mut flags: u64 = 0;

        btrfs_ioctl(fs.as_fd(), BTRFS_IOC_SUBVOL_GETFLAGS, &mut flags)
            .map(|_| flags & BTRFS_SUBVOL_RDONLY != 0)
    }

    /// See [super::set_readonly()]
    pub fn set_readonly<R: AsFd>(fs: R, readonly: bool) -> IoResult<()>
    {
        let mut flags: u64 = 0;

        btrfs_ioctl(fs.as_fd(), BTRFS_IOC_SUBVOL_GETFLAGS, &mut flags)?;

        if BTRFS_SUBVOL_RDONLY & flags != BTRFS_SUBVOL_RDONLY * readonly as u64 {
            flags ^= BTRFS_SUBVOL_RDONLY;

            btrfs_ioctl(fs.as_fd(), BTRFS_IOC_SUBVOL_SETFLAGS, &mut flags)?;
        }

        Ok(())
    }

    /// See [super::get_path()]
    pub fn get_path<R: AsFd>(treeid: u64, fs: R) -> IoResult<OsString>
    {
        let mut pos = 128;
        let mut buf = Vec::<u8>::with_capacity(pos);
        unsafe {
            buf.set_len(pos);
        }
        let mut lookup_buf = lookup::Lookup::from(fs.as_fd());
        let mut tree_search = SearchBuilder::from(fs.as_fd())
            .tree(TreeId::RootTree)
            .item_limit(1)
            .objectid(..=treeid)
            .item_type(..=RootBackref::KEY)
            .new();

        loop {
            let item = match tree_search
                .search(|(.., off)| (off, RootBackref::KEY, 0))?
                .next()
            {
                None => return Err(ErrorKind::NotFound.into()),
                Some(item) => item,
            };
            let rootvol = item.offset() == TreeId::FsTree;
            let backref = unsafe { item.get_unchecked::<RootBackref>() };
            let name = backref.name_bytes()?;
            let lookup = lookup_buf.path_bytes(backref.dirid(), item.offset())?;
            let total_len = lookup.len() + name.len();

            while total_len + !rootvol as usize > pos {
                pos += buf.len();
                buf.extend_from_within(..);
            }
            pos -= total_len;
            unsafe {
                lookup
                    .as_ptr()
                    .copy_to_nonoverlapping(buf.as_mut_ptr().add(pos), lookup.len());
                name.as_ptr()
                    .copy_to_nonoverlapping(buf.as_mut_ptr().add(pos + lookup.len()), name.len());
            }
            if rootvol {
                buf.drain(..pos);
                if pos >= 512 {
                    buf.shrink_to_fit();
                }
                return Ok(OsString::from_vec(buf));
            }
            pos -= 1;
            buf[pos] = b'/';
        }
    }
}
