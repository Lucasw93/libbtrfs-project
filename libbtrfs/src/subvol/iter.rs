//! NOTE:
//! This module will likley be removed in a future version as it is not constitant of the design
//! goals of the library. To iterate over subvolumes in a btrfs filesystme see the examples in
//! [`crate::subvol::get_rootref`] or [`crate::tree_search`].
use super::{btrfs_ioctl, ErrorKind, IoResult, SearchBuilder, SubvolInfo};
use crate::{
    bindings::{
        btrfs_ioctl_get_subvol_rootref_args, BTRFS_IOC_GET_SUBVOL_ROOTREF, BTRFS_ROOT_ITEM_KEY,
        BTRFS_ROOT_REF_KEY,
    },
    lookup::{Lookup, UserLookup},
    subvol,
    tree_search::tree_item::{RootRef, TreeItemName},
    tree_search::TreeId,
    tree_search::{SearchKeyBuilder, TreeSearch},
    util::{subvol_info_args_from_root_item, OptionFd},
    Flags,
};
use std::os::fd::AsFd;
use std::{
    collections::VecDeque,
    fs::File,
    os::unix::{fs::OpenOptionsExt, io::AsRawFd},
    path::{Path, MAIN_SEPARATOR_STR as SEP},
};

/// Entries returned by [`Iter`]
pub struct SubvolEntry
{
    treeid: u64,
    name: String,
    parent_id: u64,
    dirid: u64,
    path: Option<String>,
    info: Option<Box<SubvolInfo>>,
}

impl SubvolEntry
{
    /// Id of this subvolume
    pub fn treeid(&self) -> u64
    {
        self.treeid
    }

    /// Name for this subvolume
    pub fn name(&self) -> &str
    {
        &self.name
    }

    /// Id of the subvolume which contains this subvolume
    pub fn parent_id(&self) -> u64
    {
        self.parent_id
    }

    /// Inode of the directory containing this subvolume
    pub fn dirid(&self) -> u64
    {
        self.dirid
    }

    /// Path for this subvoume relative to toplevel subvolume given to the iterator. This is `top`
    /// for privileged or the root subvolume referenced by `pathname` for unprivileged
    ///
    /// # Panics
    ///
    /// This function panics if [`Opt::GET_PATH`] flag was no provided
    pub fn path(&self) -> &str
    {
        self.path.as_ref().expect("GET_PATH Flag not provided")
    }

    /// [`SubvolInfo`] for this subvolume
    ///
    /// # Panics
    ///
    /// This function panics if [`Opt::GET_INFO`] flag was not provided
    pub fn info(&self) -> &SubvolInfo
    {
        self.info.as_ref().expect("GET_INFO Flag not provided")
    }
}

/// Iterator over subvolumes in a btrfs filesystem
///
/// Returned by the [`walk`] function and yields instances of
/// <code>[std::io::Result]<[SubvolEntry]></code>
///
/// # Notes
///
/// **Requires CAP_SYS_ADIMN capabilities**
///
/// For an iterator that can be called by unprivileged processes see [`IterUser`]
///
/// # Panics
///
/// The iterator will panic if it encounters invalid UTF-8
pub struct Iter<'r>
{
    stack: Vec<SubvolEntry>,
    flags: Flags,
    insert_ref: fn(&mut VecDeque<SubvolEntry>, SubvolEntry),

    tree_search: TreeSearch<'r>,
    opt_fd: OptionFd<'r>,
}

impl<'r> Iter<'r>
{
    const fn opt_ordering(flags: &Flags) -> fn(&mut VecDeque<SubvolEntry>, SubvolEntry)
    {
        if flags.contains(Flags::DESCENDING) {
            VecDeque::push_back
        } else {
            VecDeque::push_front
        }
    }

    fn start_boxed_subvol_info(
        treeid: u64,
        parent_id: u64,
        dirid: u64,
        name: &[u8],
    ) -> Box<SubvolInfo>
    {
        let mut args = Box::<SubvolInfo>::new_zeroed();
        let p = unsafe { args.as_mut_ptr().as_mut().unwrap() };

        p.0.parent_id = parent_id;
        p.0.treeid = treeid;
        p.0.dirid = dirid;
        copy_bytes_to_slice!(name, p.0.name);

        unsafe { args.assume_init() }
    }

    fn finish_boxed_subvol_info(&mut self, info: &mut SubvolInfo) -> IoResult<()>
    {
        match self
            .tree_search
            .item_limit(1)
            .item_type(..=BTRFS_ROOT_ITEM_KEY)
            .search(|_| None)?
            .next()
        {
            Some(item) => {
                subvol_info_args_from_root_item(unsafe { item.get_unchecked() }, &mut info.0)
            }
            None => return Err(ErrorKind::NotFound.into()),
        };

        Ok(())
    }

    fn new_internal(top: u64, opt_fd: OptionFd<'r>, flags: Flags) -> IoResult<Self>
    {
        let insert_ref = Self::opt_ordering(&flags);
        let parent_id = top;

        let mut tree_search = SearchBuilder::from(opt_fd.clone())
            .tree(TreeId::RootTree)
            .item_limit(u32::MAX)
            .objectid(..=top)
            .item_type(..=BTRFS_ROOT_REF_KEY)
            .new();

        let items = tree_search.search(|_| None)?;

        let mut ref_buf = VecDeque::with_capacity(items.len());

        for item in items {
            let treeid = item.offset();
            let rref = unsafe { item.get_unchecked::<RootRef>() };
            let dirid = rref.dirid();
            let name = rref.name_str()?.to_string();

            let path = flags.contains(Flags::GET_PATH).then(|| name.clone());

            let info = flags
                .contains(Flags::GET_INFO)
                .then(|| Self::start_boxed_subvol_info(treeid, parent_id, dirid, name.as_bytes()));

            insert_ref(
                &mut ref_buf,
                SubvolEntry { treeid, name, parent_id, dirid, path, info },
            )
        }

        Ok(Self {
            opt_fd,
            tree_search,
            flags,
            insert_ref,
            stack: ref_buf.into(),
        })
    }
}

impl Iterator for Iter<'_>
{
    type Item = IoResult<SubvolEntry>;

    fn next(&mut self) -> Option<Self::Item>
    {
        let mut top = self.stack.pop()?;
        let parent_id = top.treeid;

        match self
            .tree_search
            .item_limit(u32::MAX)
            .objectid(..=parent_id)
            .item_type(..=BTRFS_ROOT_REF_KEY)
            .search(|_| None)
        {
            Err(e) => return Some(Err(e)),
            Ok(items) => {
                let mut ref_buf = VecDeque::with_capacity(items.len());
                let mut lu_buf = Lookup::from(self.opt_fd.as_fd());

                for item in items {
                    let treeid = item.offset();
                    let rref = unsafe { item.get_unchecked::<RootRef>() };
                    let dirid = rref.dirid();
                    let name = try_or_some_err!(rref.name_str()).to_string();

                    let path = match top.path {
                        None => None,
                        Some(ref top_path) => (top_path.clone()
                            + SEP
                            + try_or_some_err! {
                                lu_buf.path_str(dirid, parent_id)
                            }
                            .as_ref()
                            + &name)
                            .into(),
                    };

                    let info = self.flags.contains(Flags::GET_INFO).then(|| {
                        Self::start_boxed_subvol_info(treeid, parent_id, dirid, name.as_bytes())
                    });

                    (self.insert_ref)(
                        &mut ref_buf,
                        SubvolEntry { treeid, name, parent_id, dirid, path, info },
                    )
                }
                self.stack.extend(ref_buf)
            }
        }

        if let Some(ref mut info) = top.info {
            if let Err(e) = self.finish_boxed_subvol_info(info) {
                return Some(Err(e));
            }
        }

        Some(Ok(top))
    }
}

/// Iterator over subvolumes in a btrfs filesystem
///
/// Returned by the [`walk_user`] function and yields instances of
/// <code>[std::io::Result]<[SubvolEntry]></code>. Unlike [`Iter`] can be called by unprivileged
/// processes
///
/// # Panics
///
/// The iterator will panic if it encounters invalid UTF-8
pub struct IterUser<'r>
{
    fd: OptionFd<'r>,
    args: btrfs_ioctl_get_subvol_rootref_args,
    stack: Vec<SubvolEntry>,
    flags: Flags,
    insert_ref: fn(&mut VecDeque<SubvolEntry>, SubvolEntry),
}

impl<'r> IterUser<'r>
{
    fn new_internal(fd: OptionFd<'r>, flags: Flags) -> IoResult<IterUser<'r>>
    {
        let insert_ref = Iter::opt_ordering(&flags);

        let parent_id = Lookup::from(fd.as_fd()).treeid()?;

        let mut args: btrfs_ioctl_get_subvol_rootref_args =
            unsafe { std::mem::MaybeUninit::zeroed().assume_init() };

        btrfs_ioctl(fd.as_fd(), BTRFS_IOC_GET_SUBVOL_ROOTREF, &mut args)?;
        let num_items = args.num_items as usize;
        let mut ref_buf = VecDeque::with_capacity(num_items);
        let mut lu_buf = UserLookup::from(fd.as_fd());

        for rref in args.rootref.iter().take(num_items) {
            let (lookup, name) = {
                match lu_buf.path_str(rref.dirid, rref.treeid) {
                    // The IOC_LOOKUP_USER ioctl will fail with EACCES if `treeid` and `dirid` are
                    // not contained within the directory that `fd` refers to.
                    //
                    // This only happens if `fd` does not refer to a subvolume root. Ignoring this
                    // error allow findings subvolumes below any directory not just subvolumes
                    //
                    Err(e) if e.raw_os_error() == Some(libc::EACCES) => continue,
                    ret => ret?,
                }
            };
            // Add a null byte to the end of each path so it can be used for openat.
            // Remove it after the call.
            let path = Some(lookup.to_string() + name.as_ref() + "\0");
            let name = name.to_string();

            insert_ref(
                &mut ref_buf,
                SubvolEntry {
                    name,
                    parent_id,
                    path,
                    treeid: rref.treeid,
                    dirid: rref.dirid,
                    info: None,
                },
            )
        }

        Ok(Self {
            stack: ref_buf.into(),
            fd,
            args,
            flags,
            insert_ref,
        })
    }
}

impl<'f> Iterator for IterUser<'f>
{
    type Item = IoResult<SubvolEntry>;

    fn next(&mut self) -> Option<Self::Item>
    {
        let mut top = self.stack.pop()?;
        let parent_id = top.treeid;
        let parent_path = top.path.as_mut().unwrap();

        let _close_res = match syscall!(unsafe {
            openat(
                self.fd.as_raw_fd(),
                parent_path.as_ptr().cast(),
                libc::O_RDONLY | libc::O_DIRECTORY,
            )
        }) {
            Err(e) => return Some(Err(e)),
            Ok(raw_fd) => {
                use std::os::fd::FromRawFd;

                let fd = unsafe { std::os::fd::OwnedFd::from_raw_fd(raw_fd) };

                // remove null byte regardless of path_opt so path of refs are correct
                parent_path.truncate(parent_path.len() - 1);

                top.info = if self.flags.contains(Flags::GET_INFO) {
                    try_or_some_err! {
                        subvol::io::get_boxed_info(fd.as_fd())
                    }
                    .into()
                } else {
                    None
                };

                self.args.min_treeid = 0;
                if let Err(e) =
                    btrfs_ioctl(fd.as_fd(), BTRFS_IOC_GET_SUBVOL_ROOTREF, &mut self.args)
                {
                    return Some(Err(e));
                }
                let num_items = self.args.num_items as usize;
                let mut ref_buf = VecDeque::with_capacity(num_items);
                let mut lu_buf = UserLookup::from(fd.as_fd());

                for rref in self.args.rootref.iter().take(num_items) {
                    let (lookup, name) = match lu_buf.path_str(rref.dirid, rref.treeid) {
                        Err(e) => return Some(Err(e)),
                        Ok(tup) => tup,
                    };
                    let path =
                        Some(parent_path.clone() + SEP + lookup.as_ref() + name.as_ref() + "\0");
                    let name = name.to_string();

                    (self.insert_ref)(
                        &mut ref_buf,
                        SubvolEntry {
                            name,
                            parent_id,
                            path,
                            dirid: rref.dirid,
                            treeid: rref.treeid,
                            info: None,
                        },
                    )
                }
                self.stack.extend(ref_buf);
            }
        };

        if !self.flags.contains(Flags::GET_PATH) {
            top.path = None
        }

        Some(Ok(top))
    }
}

/// Returns an iterator that will walk subvolumes in a btrfs filesystem tree
///
/// The iterator returns all subvolumes below the subvolume referenced by `top`, wich must be a
/// subvolume in a btrfs filesystem referenced by `fs`. The iterator can be customized with the
/// options provided to `flags`. Calls to next will yeild instances of
/// <code>[std::io::Result]<[SubvolEntry]></code>
///
/// # Flags
///
/// Full list of available flags:
///
/// * [`Flags::ASCENDING `]
///
/// Subvolumes are returned in ascending order by treeid for each subvolume referencing a given root.
/// This is the default ordering and exists for documentation purposes. Passing this flag has no
/// effect
///
/// * [`Flags::DESCENDING `]
///
/// Subvolumes are returned in descending order by treeid for each subvolume referencing a given root
///
/// * [`Flags::GET_PATH`]
///
/// Get the subvolume path for each subvolume. [`SubvolEntry::path`] will panic if this flag is not
/// provided
///
/// * [`Flags::GET_INFO `]
///
/// Get [`SubvolInfo`] for each subvolume. [`SubvolEntry::info`] will panic if this flag is not
/// provided
///
/// Note that this option will likley change in the future because the [`SubvolInfo`] structure stores
/// the name field as an array not a dynamically allocated object which is not memory efficent
/// for members of a collection. Additionally some fields are redundant.
///
/// # Notes
///
/// **Requires CAP_SYS_ADIMN capabilities**
///
/// # Panics
///
/// The iterator will panic if it encounters invalid UTF-8
pub fn walk<'a, P: AsRef<Path>>(top: u64, fs: P, flags: Flags) -> IoResult<Iter<'a>>
{
    let f = File::open(fs)?;

    Iter::new_internal(top, OptionFd::Owned(f.into()), flags)
}

/// Returns an iterator that will walk subvolumes in a btrfs filesystem tree
///
/// The iterator returns all subvolumes below the directory referenced by `pathname`. The
/// iterator can be customized with the options provided to `flags`. Calls to next will yeild
/// instances of <code>[std::io::Result]<[SubvolEntry]></code>
///
/// # Flags
///
/// For a full list of available flags see: [`walk`]
///
/// # Errors
///
/// * [`ErrorKind::NotADirectory`]
///
/// `pathname` does not refer to a directory
///
/// Note that in the raw file descriptor version of this function (see [`io::walk_user`]) it is not
/// an error if the raw file descriptor does not refer to a directory, however no subvolumes will
/// be returned
///
/// # Panics
///
/// The iterator will panic if it encounters invalid UTF-8
pub fn walk_user<'a, P: AsRef<Path>>(pathname: P, flags: Flags) -> IoResult<IterUser<'a>>
{
    let fd = File::options()
        .read(true)
        .custom_flags(libc::O_DIRECTORY)
        .open(pathname)?;

    IterUser::new_internal(OptionFd::Owned(fd.into()), flags)
}

pub mod io
{
    use super::*;
    use std::os::fd::BorrowedFd;

    /// See [super::walk()]
    pub fn walk<'a>(top: u64, fd: BorrowedFd<'a>, flags: Flags) -> IoResult<Iter<'a>>
    {
        Iter::new_internal(top, OptionFd::Borrowed(fd), flags)
    }

    /// See [super::walk_user()]
    pub fn walk_user<'a>(fd: BorrowedFd<'a>, flags: Flags) -> IoResult<IterUser<'a>>
    {
        IterUser::new_internal(OptionFd::Borrowed(fd), flags)
    }
}
