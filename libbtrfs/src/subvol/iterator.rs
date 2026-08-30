use super::{AsFd, AsRawFd, File, MAIN_SEPARATOR, MaybeUninit, Path, SubvolInfo, btrfs_ioctl};
use crate::bindings::{BTRFS_IOC_GET_SUBVOL_ROOTREF, btrfs_ioctl_get_subvol_rootref_args};
use crate::{
    Flags,
    ffi::{UnixPath, UnixPathBuf},
};
use std::{io, mem::ManuallyDrop, os::fd::FromRawFd};

#[derive(Clone, Copy, PartialEq)]
enum Color
{
    Grey,
    Black,
    PreOrd,
}

enum Ent<R>
{
    Vol(u64, u64, File, Color, Option<UnixPathBuf>),
    RootVol(u64, R),
}

struct Iter<R>
{
    stack: Vec<Ent<R>>,
    args: MaybeUninit<btrfs_ioctl_get_subvol_rootref_args>,
    flags: Flags,
    path_buffer: Vec<u8>,
}

impl<R: AsFd> Iter<R>
{
    const PATH_BUF_SZ: usize = 256;

    fn push_refs<'a, 'b: 'a, F: AsFd + AsRawFd>(
        &'b mut self,
        parent_id: u64,
        f: F,
        is_root: bool,
        color: Color,
        path: Option<Option<&'a UnixPath>>,
    ) -> io::Result<()>
    {
        let args = unsafe {
            self.args.as_mut_ptr().write_bytes(0, 1);
            self.args.assume_init_mut()
        };
        let mut path_buf = crate::lookup::UserLookup::from(f.as_fd());

        let iter = btrfs_ioctl(f.as_fd(), BTRFS_IOC_GET_SUBVOL_ROOTREF, args)
            .map(|_| args.rootref.into_iter().take(args.num_items as usize))?;

        let refs = if self.flags.contains(Flags::DESCENDING) {
            either::Either::Left(iter)
        } else {
            // Entries are returned is the reverse order that they get pushed on the stack.
            // To return entries in ascending order (of treeid) we must reverse.
            either::Either::Right(iter.rev())
        };

        for rref in refs {
            let (lookup, name) = {
                match path_buf.path_str(rref.dirid, rref.treeid) {
                    // The IOC_LOOKUP_USER ioctl will fail with EACCES if `treeid` and `dirid` are
                    // not contained within the directory that `fd` refers to.
                    //
                    // This only happens if `fd` does not refer to a subvolume root. Ignoring this
                    // error allow findings subvolumes below any directory not just subvolumes
                    Err(e) => {
                        if is_root && e.raw_os_error() == Some(libc::EACCES) {
                            continue;
                        } else {
                            return Err(e);
                        }
                    }
                    Ok((lookup, name)) => (lookup.to_bytes(), name.to_bytes_with_nul()),
                }
            };

            let (c_path, ent_path) = match path {
                Some(unix_path) => {
                    let base = unix_path
                        .map(UnixPath::to_bytes_with_nul)
                        .unwrap_or_default();
                    let mut v = Vec::with_capacity(base.len() + lookup.len() + name.len());

                    if !base.is_empty() {
                        v.extend_from_slice(base);
                        v[base.len() - 1] = MAIN_SEPARATOR as u8;
                    }
                    v.extend_from_slice(lookup);
                    v.extend_from_slice(name);
                    (v[base.len()..].as_ptr().cast(), unsafe {
                        Some(UnixPathBuf::from_vec_with_nul_unchecked(v))
                    })
                }
                None => {
                    self.path_buffer.clear();
                    self.path_buffer.extend_from_slice(lookup);
                    self.path_buffer.extend_from_slice(name);
                    (self.path_buffer.as_ptr().cast(), None)
                }
            };

            let ent_fd = unsafe {
                syscall!(openat(f.as_raw_fd(), c_path, libc::O_RDONLY))
                    .map(|f| File::from_raw_fd(f))?
            };
            self.stack
                .push(Ent::Vol(rref.treeid, parent_id, ent_fd, color, ent_path));
        }

        Ok(())
    }
}

impl<R: AsFd> Iterator for Iter<R>
{
    type Item = std::io::Result<SubvolItem>;

    fn next(&mut self) -> Option<Self::Item>
    {
        while let Some(ent) = self.stack.pop() {
            match ent {
                Ent::RootVol(treeid, f) => {
                    let color = if self.flags.contains(Flags::POST_ORDER) {
                        Color::Grey
                    } else {
                        Color::PreOrd
                    };
                    let path = self.flags.contains(Flags::GET_PATH).then_some(None);

                    try_or_some_err! {
                        self.push_refs(treeid, f.as_fd(), true, color, path)
                    }
                }
                Ent::Vol(treeid, parent_id, f, Color::Grey, path) => {
                    // SAFTEY: `tmp_f`, and `tmp_path` are both shallow clones whose liftimes exists
                    // only for the scope of this match block. They are required because both are
                    // needed for the call to `push_refs` but will get moved when they are pushed
                    // to the stack. Neither have destructors and their validity is tied to to the
                    // entry that gets pushed to the stack.
                    let tmp_f = ManuallyDrop::new(unsafe { File::from_raw_fd(f.as_raw_fd()) });
                    let tmp_path = path
                        .as_ref()
                        .map(|path_buf| unsafe { Some(UnixPath::from_ptr(path_buf.as_ptr())) });

                    self.stack
                        .push(Ent::Vol(treeid, parent_id, f, Color::Black, path));
                    try_or_some_err! {
                        self.push_refs(treeid,  tmp_f.as_fd(), false, Color::Grey, tmp_path)
                    }
                }
                Ent::Vol(treeid, parent_id, f, color, path) => {
                    if color == Color::PreOrd {
                        try_or_some_err! {
                            self.push_refs(
                                treeid,
                                f.as_fd(),
                                false,
                                Color::PreOrd,
                                path.as_deref().map(Some),
                            )
                        }
                    }
                    return Some(Ok(SubvolItem { f, treeid, parent_id, dirid: 100, path }));
                }
            }
        }

        None
    }
}

/// Items returned by the the subvolume iterator.
///
/// See the [`iter()`] function for more information.
pub struct SubvolItem
{
    f: File,
    treeid: u64,
    parent_id: u64,
    dirid: u64,

    path: Option<UnixPathBuf>,
}

impl AsFd for SubvolItem
{
    fn as_fd(&self) -> std::os::fd::BorrowedFd<'_>
    {
        self.f.as_fd()
    }
}

impl SubvolItem
{
    /// Path to this subvolume relative to the path argument given to [`iter()`].
    ///
    /// # Panics
    ///
    /// Panics if [`Flags::GET_PATH`] was not provided to [`iter()`].
    pub fn path(&self) -> &crate::ffi::UnixPath
    {
        self.path
            .as_deref()
            .expect("Missing flag: libbtrfs::Flags::GET_PATH")
    }

    /// Returns a [`SubvolInfo`] struct for this subvolume.
    pub fn get_info(&self) -> io::Result<SubvolInfo>
    {
        super::fd::get_info(self.f.as_fd())
    }

    /// Returns a heap allocated [`SubvolInfo`] struct.
    pub fn get_boxed_info(&self) -> io::Result<Box<SubvolInfo>>
    {
        super::fd::get_boxed_info(self.f.as_fd())
    }

    /// Inode where this subvolume is rooted.
    pub fn dirid(&self) -> u64
    {
        self.dirid
    }

    /// ID of this subvolume.
    pub const fn treeid(&self) -> u64
    {
        self.treeid
    }

    /// ID of the parent subvolume.
    pub const fn parent_id(&self) -> u64
    {
        self.parent_id
    }
}

/// Returns a iterators over subvolumes in a btrfs filesystem.
///
/// This function will return an iterator that will iterate over all subvolumes below `path`. The
/// `flags` argument allows custimize the iterator, see the flags section for more information.
///
/// # Flags
///
/// [`Flags::GET_PATH`]
///
/// > When this option is provided the iterator will get the path for every subvolume that it
/// finds. If this flag is missing the [`SubvolItem::path()`] function will panic.
///
/// [`Flags::PRE_ORDER`]
///
/// > Subvolmes are returned in pre order, ie. parents subvolumes are visited before their
/// children. This is the default, passing the flag is no-op.
///
/// [`Flags::POST_ORDER`]
///
/// > Subvolumes are returned in post order, ie. bottom up traversal.
///
/// [`Flags::ASCENDING`]
///
/// > For each parent subvolume, its children are returned by ID in ascending order. This is the
/// default, passing this flag is a no-op.
///
/// [`Flags::DESCENDING`]
///
/// > For each parent subvolume, its children are returned by ID in descending order. This is the
/// default, passing this flag is a no-op.
pub fn iter<P: AsRef<Path>>(
    path: P,
    flags: Flags,
) -> std::io::Result<impl Iterator<Item = std::io::Result<SubvolItem>>>
{
    File::open(path).and_then(|f| fd::iter(f, flags))
}

pub mod fd
{
    use super::*;

    /// See [super::iter()]
    pub fn iter<R: AsFd>(
        r: R,
        flags: Flags,
    ) -> std::io::Result<impl Iterator<Item = std::io::Result<SubvolItem>>>
    {
        let treeid = crate::lookup::Lookup::from(r.as_fd()).treeid()?;

        let path_buffer = flags
            .contains(Flags::GET_PATH)
            .then(|| Vec::with_capacity(Iter::<R>::PATH_BUF_SZ))
            .unwrap_or_default();

        Ok(Iter {
            flags,
            path_buffer,
            stack: vec![Ent::RootVol(treeid, r)],
            args: MaybeUninit::uninit(),
        })
    }
}
