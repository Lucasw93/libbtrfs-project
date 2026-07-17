use super::*;
use crate::{
    KernelStr,
    bindings::{
        BTRFS_IOC_GET_SUBVOL_ROOTREF, btrfs_ioctl_get_subvol_rootref_args,
        btrfs_ioctl_get_subvol_rootref_args__bindgen_ty_1,
    },
};

/// Subvolume ID and dirid for a subvolume that references a root subvolume.
///
/// This struct is the first argument received for the [`get_rootref()`] callback.
#[repr(transparent)]
pub struct SubvolRootRef(btrfs_ioctl_get_subvol_rootref_args__bindgen_ty_1);

impl SubvolRootRef
{
    /// Subvolume ID
    pub const fn treeid(&self) -> u64
    {
        self.0.treeid
    }

    /// Inode of directory where this subvolume is rooted
    pub const fn dirid(&self) -> u64
    {
        self.0.dirid
    }
}

/// Find all subvolume's referenceing a root subvolume.
///
/// Find all subvolume referenced by `subvol`. `subvol` does  not need to
/// be a subvolume root but if not, then  subvolumes above `subvol` will not be found.
///
/// `f` is a callback that provides access to the [`SubvolRootRef`] struct as the
/// first argument. The second argument is the lookup path to the subvolume relative to `subvol`
/// and the last argument is the subvolume name. The last two argument are essentally the
/// destructured tuple returned by [`crate::lookup::UserLookup::path_str`].
///
/// # Example
///
/// This function will recursively walk the subvolume tree and find all subvolumes below `path`.
///
/// ```no_run
/// use libbtrfs::subvol::get_rootref;
///
/// fn walk(path: &str) -> std::io::Result<()>
/// {
///     let mut buf = path.to_string();
///     if !buf.ends_with('/') {
///         buf.push('/')
///     }
///     let buf_len = buf.len();
///
///     get_rootref(path, |rref, lookup, name| {
///         buf.push_str(&lookup);
///         buf.push_str(&name);
///
///         println!("ID {}: {}", rref.treeid(), buf);
///
///         walk(&buf)?;
///         buf.truncate(buf_len);
///
///         Ok(())
///     })
/// }
/// ```
pub fn get_rootref<P, F>(subvol: P, f: F) -> IoResult<()>
where
    P: AsRef<Path>,
    F: FnMut(SubvolRootRef, KernelStr<'_>, KernelStr<'_>) -> IoResult<()>,
{
    File::open(subvol).and_then(|r| io::get_rootref(r, f))
}

pub mod io
{
    use super::*;

    /// see [super::get_rootref()]
    pub fn get_rootref<R, F>(resource: R, mut f: F) -> IoResult<()>
    where
        R: AsFd,
        F: FnMut(SubvolRootRef, KernelStr<'_>, KernelStr<'_>) -> IoResult<()>,
    {
        let mut args: btrfs_ioctl_get_subvol_rootref_args =
            unsafe { MaybeUninit::zeroed().assume_init() };

        btrfs_ioctl(resource.as_fd(), BTRFS_IOC_GET_SUBVOL_ROOTREF, &mut args)?;
        let mut lubuf = crate::lookup::UserLookup::from(resource.as_fd());

        for rref in args.rootref.into_iter().take(args.num_items as usize) {
            let rref = SubvolRootRef(rref);
            let (lookup, name) = lubuf.path_str(rref.dirid(), rref.treeid())?;

            f(rref, lookup, name)?;
        }

        Ok(())
    }
}
