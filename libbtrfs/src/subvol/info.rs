use super::*;
use crate::tree_search::tree_item::RootRef;
use crate::util::KernelStr;
use crate::util::subvol_info_args_from_root_item;
use std::ptr::{write, write_bytes};

/// Describes time in seonds and nanoseconds
#[repr(transparent)]
pub struct Timespec(btrfs_ioctl_timespec);

impl Timespec
{
    /// Seconds
    pub const fn sec(&self) -> u64
    {
        self.0.sec
    }

    /// Nanoseconds
    pub const fn nsec(&self) -> u32
    {
        self.0.nsec
    }
}

/// Information about a btrfs subvolume root
///
/// This structure is returned by the [`get_info`] or [`get_info_by_id`] functions and represents
/// information about a btrfs subvolume root
#[repr(transparent)]
pub struct SubvolInfo(pub(super) btrfs_ioctl_get_subvol_info_args);

impl SubvolInfo
{
    /// Id of this subvolume
    pub const fn treeid(&self) -> u64
    {
        self.0.treeid
    }

    /// Name of this subvolume
    pub fn name_str(&self) -> KernelStr<'_>
    {
        unsafe { CStr::from_ptr(self.0.name.as_ptr().cast()).to_string_lossy() }
    }

    /// Name of this subvolume
    pub const fn name_bytes(&self) -> &[u8]
    {
        unsafe { CStr::from_ptr(self.0.name.as_ptr()).to_bytes() }
    }

    /// Id of the subvolume which contains this subvolume
    pub const fn parent_id(&self) -> u64
    {
        self.0.parent_id
    }

    /// Inode of the directory which contains this subvolume
    pub const fn dirid(&self) -> u64
    {
        self.0.dirid
    }

    /// Latest transaction id of this subvolume
    pub const fn generation(&self) -> u64
    {
        self.0.generation
    }

    /// Flags for this subvolume
    pub const fn flags(&self) -> u64
    {
        self.0.flags
    }

    /// Readonly status for this subvolume
    pub const fn readonly(&self) -> bool
    {
        self.0.flags & BTRFS_SUBVOL_RDONLY != 0
    }

    /// UUID of this subvolume
    pub const fn uuid(&self) -> Uuid
    {
        Uuid::from_bytes(self.0.uuid)
    }

    /// UUID of the subvolume of which this subvolume is a snapshot. This will be nil for
    /// non-snapshot subvolumes
    ///
    /// See [`Uuid::is_nil`]
    pub const fn parent_uuid(&self) -> Uuid
    {
        Uuid::from_bytes(self.0.parent_uuid)
    }

    /// UUID of the subvolume from which this subvolume was received. This will be nil for
    /// non-received subvolumes
    ///
    /// See [`Uuid::is_nil`]
    pub const fn received_uuid(&self) -> Uuid
    {
        Uuid::from_bytes(self.0.received_uuid)
    }

    /// transaction ID indicating when change happened
    pub const fn ctransid(&self) -> u64
    {
        self.0.ctransid
    }

    /// transaction ID indicating when create happened
    pub const fn otransid(&self) -> u64
    {
        self.0.otransid
    }

    /// transaction ID indicating when send happened
    pub const fn stransid(&self) -> u64
    {
        self.0.stransid
    }

    /// transaction ID indicating when receive happened
    pub const fn rtransid(&self) -> u64
    {
        self.0.rtransid
    }

    /// Time corresponding to ctransid
    pub const fn ctime(&self) -> Timespec
    {
        Timespec(self.0.ctime)
    }

    /// Time corresponding to otransid
    pub const fn otime(&self) -> Timespec
    {
        Timespec(self.0.otime)
    }

    /// Time corresponding to stransid
    pub const fn stime(&self) -> Timespec
    {
        Timespec(self.0.stime)
    }

    /// Time corresponding to rtransid
    pub const fn rtime(&self) -> Timespec
    {
        Timespec(self.0.rtime)
    }
}

/// Return a [`SubvolInfo`] struct.
///
/// This function returns information about the subvolume that contains `pathname`, which can be a
/// subvolume, directory, or regular file in a btrfs filesystem
///
/// # Errors
///
/// [`ErrorKind::NotFound`]
///
/// > No file exists at `pathname`
pub fn get_info<P: AsRef<Path>>(pathname: P) -> IoResult<SubvolInfo>
{
    File::open(pathname).and_then(io::get_info)
}

/// Return a [`SubvolInfo`] struct that has been allocated on the heap.
///
/// This function is equivelent to [`get_info`] except memory is allocated on the heap instead of
/// the stack.
pub fn get_boxed_info<P: AsRef<Path>>(pathname: P) -> IoResult<Box<SubvolInfo>>
{
    File::open(pathname).and_then(io::get_boxed_info)
}

/// Query information about a btrfs subvolume by its subvolume id
///
/// # Errors
///
/// [`ErrorKind::NotFound`]
///
/// > `treeid` is not a valid subvolume id
///
/// # Notes
///
/// **Requires CAP_SYS_ADMIN capabilities**
pub fn get_info_by_id<P: AsRef<Path>>(treeid: u64, fs: P) -> IoResult<SubvolInfo>
{
    File::open(fs).and_then(|f| io::get_info_by_id(treeid, f))
}

pub mod io
{
    use super::*;
    use crate::bindings::{BTRFS_FS_TREE_OBJECTID, BTRFS_ROOT_BACKREF_KEY, BTRFS_ROOT_ITEM_KEY};

    /// See [super::get_info()]
    pub fn get_info<R: AsFd>(fd: R) -> IoResult<SubvolInfo>
    {
        let mut info_args: MaybeUninit<SubvolInfo> = MaybeUninit::zeroed();

        btrfs_ioctl(fd, BTRFS_IOC_GET_SUBVOL_INFO, info_args.as_mut_ptr())
            .map(|_| unsafe { info_args.assume_init() })
    }

    /// See [super::get_boxed_info()]
    pub fn get_boxed_info<R: AsFd>(fd: R) -> IoResult<Box<SubvolInfo>>
    {
        let mut info_args: Box<MaybeUninit<SubvolInfo>> = Box::new_zeroed();

        btrfs_ioctl(fd, BTRFS_IOC_GET_SUBVOL_INFO, info_args.as_mut_ptr())
            .map(|_| unsafe { info_args.assume_init() })
    }

    /// See [super::get_info_by_id()]
    pub fn get_info_by_id<R: AsFd>(treeid: u64, fd: R) -> IoResult<SubvolInfo>
    {
        let mut info = MaybeUninit::<btrfs_ioctl_get_subvol_info_args>::uninit();
        let mut got_root_item = false;
        let mut got_root_ref = treeid == BTRFS_FS_TREE_OBJECTID;
        let mut tree_search = SearchBuilder::from(fd.as_fd())
            .tree(TreeId::RootTree)
            .item_limit(u32::MAX)
            .objectid(..=treeid)
            .item_type(
                { BTRFS_ROOT_ITEM_KEY }..{
                    if got_root_ref {
                        BTRFS_ROOT_ITEM_KEY
                    } else {
                        BTRFS_ROOT_BACKREF_KEY
                    }
                },
            )
            .new();

        let info_ptr = info.as_mut_ptr();

        while !got_root_item && !got_root_ref {
            let items = tree_search.search(|_| None)?;

            if items.len() == 0 {
                return Err(ErrorKind::NotFound.into());
            }
            for item in items {
                match item.ty() {
                    BTRFS_ROOT_ITEM_KEY => {
                        subvol_info_args_from_root_item(unsafe { item.get_unchecked() }, info_ptr);
                        got_root_item = true;
                    }
                    BTRFS_ROOT_BACKREF_KEY => unsafe {
                        let subv_name: *mut u8 = (&raw mut (*info_ptr).name).cast();
                        let rr = item.get_unchecked::<RootRef>();
                        let rr_name = rr.name_bytes()?;

                        rr_name
                            .as_ptr()
                            .copy_to_nonoverlapping(subv_name, rr_name.len());
                        subv_name.add(rr_name.len()).write(0);

                        write(&raw mut (*info_ptr).dirid, rr.dirid());
                        write(&raw mut (*info_ptr).parent_id, item.offset());
                        got_root_ref = true;
                    },
                    _ => {}
                }
            }
        }
        unsafe {
            write(&raw mut (*info_ptr).treeid, treeid);
            write_bytes(&raw mut (*info_ptr).reserved, 0, 1);

            Ok(SubvolInfo(info.assume_init()))
        }
    }
}
