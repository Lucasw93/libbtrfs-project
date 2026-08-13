//! Btrfs tree searches.
//!
//! # Example
//!
//! The following search will find all subvolumes in the btrfs filesystem referenced by the path `/`.
//!
//! ```no_run
//! use libbtrfs::tree_search::{
//!     Query, SearchBuilder, SearchKeyBuilder, TreeId,
//!     tree_item::{RootRef, TreeItemName},
//! };
//!
//! let mut search = SearchBuilder::from_path("/")?
//!     .tree(TreeId::RootTree)
//!     .item_limit(u32::MAX)
//!     .objectid(5..u64::MAX)
//!     .build();
//!
//! loop {
//!     let items = search.query(|(objectid, ..)| (objectid + 1, 0, 0))?;
//!
//!     if items.len() == 0 {
//!         break;
//!     }
//!
//!     for item in items {
//!         if let Some(rr) = item.get::<RootRef>() {
//!             println!(
//!                 "ID {} level {} name {}",
//!                 item.offset(),
//!                 item.objectid(),
//!                 rr.name_as_os_str()?.display()
//!             )
//!         }
//!     }
//! }
//! # Ok::<(), std::io::Error>(())
//! ````
use crate::{
    bindings::{
        BTRFS_IOC_TREE_SEARCH, BTRFS_IOC_TREE_SEARCH_V2, btrfs_ioctl_search_args,
        btrfs_ioctl_search_args_v2, btrfs_ioctl_search_header, btrfs_ioctl_search_key,
    },
    util::{IoResult, btrfs_ioctl},
};
use std::{
    alloc::{Layout, alloc, dealloc, handle_alloc_error},
    fs::File,
    marker::PhantomData,
    mem::{ManuallyDrop, MaybeUninit},
    ops::{Bound, RangeBounds},
    os::fd::AsFd,
    path::Path,
    ptr::{read_unaligned, write},
};

pub mod tree_item;
use tree_item::TreeItem;

/// Primary sort key for a btrfs item. Field 0 of `DiskKey tuple.
pub type ObjectId = u64;
/// Secondary sort key for a btrfs item. Field 1 of `DiskKey tuple.
pub type Ty = u32;
/// Tertiary sort key for a btrfs item. Field 2 of `DiskKey tuple.
pub type Offset = u64;

/// Tuple that represent a btrfs sort key
pub type DiskKey = (ObjectId, Ty, Offset);

/// Set the search key from the last seen [`DiskKey`]
///
/// Return value for the closure called when the [`TreeSearch::search()`] iterator goes out of
/// scope. Sets the search key used for future calls to [`TreeSearch::search()`] based on the last
/// seen [`DiskKey`].
pub trait FromDiskKey
{
    #[doc(hidden)]
    fn as_disk_key(self) -> Option<DiskKey>;
}

impl FromDiskKey for Option<()>
{
    fn as_disk_key(self) -> Option<DiskKey>
    {
        None
    }
}

impl FromDiskKey for ()
{
    fn as_disk_key(self) -> Option<DiskKey>
    {
        None
    }
}

impl FromDiskKey for u64
{
    fn as_disk_key(self) -> Option<DiskKey>
    {
        todo!()
    }
}

impl FromDiskKey for DiskKey
{
    fn as_disk_key(self) -> Option<DiskKey>
    {
        Some(self)
    }
}

/// Increment the key to the next objectid
///
/// Returns a [`DiskKey`] with the `objectid` incremeted by one from `key`. If the objectid for `key`
/// is [`u64::MAX`] then `key` is returned unmodified.
pub fn next_objectid(key: DiskKey) -> DiskKey
{
    u64::checked_add(key.0, 1).map_or(key, |obj| (obj, 0, 0))
}

/// Increment the key to the next type
///
/// Returns a [`DiskKey`] with the `type` incremeted by one from `key`. If the type for `key` is
/// [`u8::MAX`] then the `type` is set to 0 and the `objectid` is incremeted.
pub fn next_type(key: DiskKey) -> DiskKey
{
    u8::checked_add(key.1 as u8, 1).map_or_else(|| next_type(key), |ty| (key.0, ty as u32, 0))
}

/// Increment the key to the next offset
///
/// Returns a [`DiskKey`] with the `offset` incremeted by one from `key`. If the offset for `key`
/// is [`u64::MAX`] then the `offset` is set to 0 and the `type` is incremeted.
pub fn next_offset(key: DiskKey) -> DiskKey
{
    u64::checked_add(key.2, 1).map_or_else(|| next_type(key), |off| (key.0, key.1, off))
}

struct FnOnDrop<'buf, F, K>
where
    K: FromDiskKey,
    F: FnOnce(DiskKey) -> K,
{
    f: ManuallyDrop<F>,
    key: &'buf mut btrfs_ioctl_search_key,
}

/// Contains the Query::query() method for performing tree searches.
pub trait Query
{
    /// Returns an iterator yeilding instances of [`SearchItem`].
    ///
    /// The [`ExactSizeIterator::len()`] method can be used to check how many items the search
    /// returned.
    fn query<'buf, F, K>(
        &'buf mut self,
        on_drop: F,
    ) -> IoResult<impl Iterator<Item = SearchItem<'buf>> + ExactSizeIterator>
    where
        K: FromDiskKey,
        F: FnOnce(DiskKey) -> K;
}

// ============================================================================
// Iterater. returned by the search method for both stack and heap searches

/// Iterator over items of a btrfs tree search
struct Iter<'buf, F, K>
where
    K: FromDiskKey,
    F: FnOnce(DiskKey) -> K,
{
    buffer: *const libc::c_char,
    index: usize,
    curr_offset: usize,
    prev_offset: usize,
    on_drop: FnOnDrop<'buf, F, K>,

    _phantom: PhantomData<&'buf libc::c_char>,
}

impl<F, K> Drop for Iter<'_, F, K>
where
    K: FromDiskKey,
    F: FnOnce(DiskKey) -> K,
{
    fn drop(&mut self)
    {
        let hdr = unsafe {
            self.buffer
                .add(self.prev_offset)
                .cast::<btrfs_ioctl_search_header>()
                .read_unaligned()
        };
        let callback = unsafe { ManuallyDrop::take(&mut self.on_drop.f) };

        if let Some((obj, ty, off)) = callback((hdr.objectid, hdr.type_, hdr.offset)).as_disk_key()
        {
            self.on_drop.key.min_objectid = obj;
            self.on_drop.key.min_type = ty;
            self.on_drop.key.min_offset = off;
        }
    }
}

impl<'buf, F, K> ExactSizeIterator for Iter<'buf, F, K>
where
    K: FromDiskKey,
    F: FnOnce(DiskKey) -> K,
{
}

impl<'buf, F, K> Iterator for Iter<'buf, F, K>
where
    K: FromDiskKey,
    F: FnOnce(DiskKey) -> K,
{
    type Item = SearchItem<'buf>;

    fn next(&mut self) -> Option<Self::Item>
    {
        if self.index == 0 {
            return None;
        }
        self.index -= 1;

        let item = SearchItem(
            unsafe { self.buffer.add(self.curr_offset).cast() },
            PhantomData,
        );
        self.prev_offset = self.curr_offset;
        self.curr_offset += item.total_len();

        Some(item)
    }

    fn size_hint(&self) -> (usize, Option<usize>)
    {
        (self.index, Some(self.index))
    }
}

// ============================================================================
// Item. Returned by the seach iterator

/// Items returned from a btrfs tree search.
#[derive(Clone, Copy)]
pub struct SearchItem<'buf>(
    *const btrfs_ioctl_search_header,
    PhantomData<&'buf libc::c_char>,
);

impl<'buf> SearchItem<'buf>
{
    /// Gets the object id for this item
    #[inline]
    pub const fn objectid(self) -> u64
    {
        unsafe { read_unaligned(&raw const (*self.0).objectid) }
    }

    /// Gets the key type for this item
    #[inline]
    pub const fn ty(self) -> u32
    {
        unsafe { read_unaligned(&raw const (*self.0).type_) }
    }

    /// Gets the offset for this item
    #[inline]
    pub const fn offset(self) -> u64
    {
        unsafe { read_unaligned(&raw const (*self.0).offset) }
    }

    /// Return the disk sorting key for this item
    pub const fn key(self) -> DiskKey
    {
        let hdr = unsafe { read_unaligned(&raw const (*self.0)) };

        (hdr.objectid, hdr.type_, hdr.offset)
    }

    /// Gets the transaction id for this item
    #[inline]
    pub const fn transid(self) -> u64
    {
        unsafe { read_unaligned(&raw const (*self.0).transid) }
    }

    /// Gets the data length for this item
    #[inline]
    pub const fn len(self) -> u32
    {
        unsafe { read_unaligned(&raw const (*self.0).len) }
    }

    /// Cast the item as a [`TreeItem`] without checking the type.
    #[inline]
    pub unsafe fn get_unchecked<T: TreeItem<'buf>>(self) -> T
    {
        TreeItem::from_search_unckeched(self.0.add(1).cast::<T>())
    }

    /// Attempt to cast the item as a [`TreeItem`].
    #[inline]
    pub fn get<T: TreeItem<'buf>>(self) -> Option<T>
    {
        TreeItem::from_search(unsafe { self.0.add(1).cast::<T>() }, self.ty())
    }

    #[inline]
    const fn total_len(self) -> usize
    {
        size_of::<btrfs_ioctl_search_header>() + self.len() as usize
    }
}

/// Provides the [`Self::search()`] method.
///
/// This struct is returned by the [`SearchBuilder::new()`] method.
///
/// See the [`SearchKeyBuilder`] trait for updating the search key.
pub struct TreeSearch<R: AsFd>
{
    resource: R,
    args: MaybeUninit<btrfs_ioctl_search_args>,
}

impl<'a, R: AsFd> seal::SearchKeyBuilderExt for &'a mut TreeSearch<R>
{
    #[inline(always)]
    fn get_key(&mut self) -> &mut self::btrfs_ioctl_search_key
    {
        unsafe { &mut (*self.args.as_mut_ptr()).key }
    }
}
impl<'a, R: AsFd> SearchKeyBuilder for &'a mut TreeSearch<R> {}

impl<R: AsFd> Query for TreeSearch<R>
{
    fn query<'buf, F, K>(
        &'buf mut self,
        on_drop: F,
    ) -> IoResult<impl Iterator<Item = SearchItem<'buf>> + ExactSizeIterator>
    where
        K: FromDiskKey,
        F: FnOnce(DiskKey) -> K,
    {
        let fd = self.resource.as_fd();
        let args = self.args.as_mut_ptr();
        let key = unsafe { &mut (*args).key };
        let nr_items_in = key.nr_items;

        btrfs_ioctl(fd, BTRFS_IOC_TREE_SEARCH, args)?;

        let index = key.nr_items as usize;
        key.nr_items = nr_items_in;

        Ok(Iter {
            index,
            buffer: unsafe { &raw const (*args).buf }.cast(),
            on_drop: FnOnDrop { f: ManuallyDrop::new(on_drop), key },
            curr_offset: 0,
            prev_offset: 0,
            _phantom: PhantomData,
        })
    }
}

/// Provides the [`Self::search()`] method.
///
/// This struct is returned by the [`SearchBuilder::new_boxed()`] method.
///
/// This struct is equivelent to the [`TreeSearch`] struct, except that the search buffer is
/// allocated on the heap. The buffer size is determied by the argument given to the
/// [`SearchBuilder::new_boxed()`] method.
///
/// See the [`SearchKeyBuilder`] trait for updating the search key.
pub struct BoxedTreeSearch<R: AsFd>
{
    resource: R,
    args: *mut btrfs_ioctl_search_args_v2,
}

impl<'a, R: AsFd> seal::SearchKeyBuilderExt for &'a mut BoxedTreeSearch<R>
{
    #[inline(always)]
    fn get_key(&mut self) -> &mut self::btrfs_ioctl_search_key
    {
        unsafe { &mut (*self.args).key }
    }
}

impl<'a, R: AsFd> SearchKeyBuilder for &'a mut BoxedTreeSearch<R> {}

impl<R: AsFd> Drop for BoxedTreeSearch<R>
{
    fn drop(&mut self)
    {
        unsafe {
            let size = (*self.args).buf_size as usize + size_of::<btrfs_ioctl_search_args_v2>();
            let align = align_of::<btrfs_ioctl_search_args_v2>();
            let layout = Layout::from_size_align(size, align).unwrap();

            dealloc(self.args.cast(), layout);
        }
    }
}

impl<R: AsFd> Query for BoxedTreeSearch<R>
{
    fn query<'buf, F, K>(
        &'buf mut self,
        on_drop: F,
    ) -> IoResult<impl Iterator<Item = SearchItem<'buf>> + ExactSizeIterator>
    where
        K: FromDiskKey,
        F: FnOnce(DiskKey) -> K,
    {
        let fd = self.resource.as_fd();
        let args = self.args;
        let key = unsafe { &mut (*args).key };
        let nr_items_in = key.nr_items;

        btrfs_ioctl(fd, BTRFS_IOC_TREE_SEARCH_V2, args)?;

        let index = key.nr_items as usize;
        key.nr_items = nr_items_in;

        Ok(Iter {
            index,
            buffer: unsafe { &raw const (*args).buf }.cast(),
            on_drop: FnOnDrop { f: ManuallyDrop::new(on_drop), key },
            curr_offset: 0,
            prev_offset: 0,
            _phantom: PhantomData,
        })
    }
}

// ============================================================================
// Builder. Used to construct Tree searches both TreeSearch and BoxedTreeSearch

/// Constructs either a [`TreeSearch`] or a [`BoxedTreeSearch`]
pub struct SearchBuilder<R: AsFd>
{
    key: btrfs_ioctl_search_key,
    resource: R,
}

impl<R: AsFd> seal::SearchKeyBuilderExt for SearchBuilder<R>
{
    #[inline(always)]
    fn get_key(&mut self) -> &mut self::btrfs_ioctl_search_key
    {
        &mut self.key
    }
}

impl<R: AsFd> SearchKeyBuilder for SearchBuilder<R> {}

impl SearchBuilder<File>
{
    /// Constructs a new `SearchBuilder` from a path.
    ///
    /// This is fallible, see [`SearchBuilder::new()`] for an infallible variant.
    pub fn from_path<P: AsRef<Path>>(path: P) -> IoResult<Self>
    {
        Ok(Self {
            key: Default::default(),
            resource: File::open(path)?,
        })
    }
}

impl<R: AsFd> SearchBuilder<R>
{
    /// Constructs a new `SearchBuilder` from an IO resouce
    pub fn new(resource: R) -> Self
    {
        Self { key: Default::default(), resource }
    }

    /// Build a new [`TreeSearch`].
    pub fn build(self) -> TreeSearch<R>
    {
        let mut args = MaybeUninit::<btrfs_ioctl_search_args>::uninit();
        let argp = args.as_mut_ptr();
        unsafe {
            write(&raw mut (*argp).key, self.key);
        }
        TreeSearch { args, resource: self.resource }
    }

    /// Build a new [`BoxedTreeSearch`]. `BoxedTreeSearch` contains heap allocated memory which is
    /// baed on `buf_size`.
    pub fn build_boxed(self, buf_size: u64) -> BoxedTreeSearch<R>
    {
        let size = buf_size as usize + size_of::<btrfs_ioctl_search_args_v2>();
        let align = align_of::<btrfs_ioctl_search_args_v2>();

        let layout = Layout::from_size_align(size, align).unwrap();

        let args = unsafe {
            let args = alloc(layout).cast::<btrfs_ioctl_search_args_v2>();

            if args.is_null() {
                handle_alloc_error(layout)
            }
            write(&raw mut (*args).key, self.key);
            write(&raw mut (*args).buf_size, buf_size);

            args
        };

        BoxedTreeSearch { args, resource: self.resource }
    }
}

macro_rules! tree_search_set_key_by_range {
    ($arg:ident;  $__self:ident . $get_key_fn:ident -> $min:ident | $max:ident) => {{
        match $arg.start_bound() {
            Bound::Unbounded => {
                if let Bound::Included(&b) = $arg.end_bound() {
                    $__self.$get_key_fn().$min = b;
                    $__self.$get_key_fn().$max = b;

                    return $__self;
                }
            }
            Bound::Excluded(&b) | Bound::Included(&b) => {
                $__self.$get_key_fn().$min = b;
            }
        }
        if let Bound::Included(&b) | Bound::Excluded(&b) = $arg.end_bound() {
            $__self.$get_key_fn().$max = b;
        };

        $__self
    }};
}

/// Argument for [`SearchKeyBuilder::tree()`]
///
/// Note: documenation for fields has been taken verbatum from the Linux Kernel at, `btrfs_tree.h`
#[repr(u64)]
#[derive(Clone, Copy)]
pub enum TreeId
{
    /// Holds pointers to all of the tree roots.
    RootTree = crate::bindings::BTRFS_ROOT_TREE_OBJECTID,

    /// Stores information about which extents are in use, and reference counts.
    ExtentTree = crate::bindings::BTRFS_EXTENT_TREE_OBJECTID,

    /// Chunk tree stores translations from logical -> physical block numbering.
    /// The super block points to the chunk tree.
    ChunkTree = crate::bindings::BTRFS_CHUNK_TREE_OBJECTID,

    /// Stores information about which areas of a given device are in use. One per device. The
    /// tree of tree roots points to the device tree.
    DevTree = crate::bindings::BTRFS_DEV_TREE_OBJECTID,

    /// One per subvolume, storing files and directories.
    FsTree = crate::bindings::BTRFS_FS_TREE_OBJECTID,

    /// Directory objectid inside the root tree.
    RootTreeDir = crate::bindings::BTRFS_ROOT_TREE_DIR_OBJECTID,

    /// Holds checksums of all the data extents.
    CsumTree = crate::bindings::BTRFS_CSUM_TREE_OBJECTID,

    /// Holds quota configuration and tracking.
    QuotaTree = crate::bindings::BTRFS_QUOTA_TREE_OBJECTID,

    /// For storing items that use the BTRFS_UUID_KEY* types.
    UuidTree = crate::bindings::BTRFS_UUID_TREE_OBJECTID,

    /// Tracks free space in block groups.
    FreeSpaceTree = crate::bindings::BTRFS_FREE_SPACE_TREE_OBJECTID,

    /// Holds the block group items for extent tree v2.
    BlockGroupTree = crate::bindings::BTRFS_BLOCK_GROUP_TREE_OBJECTID,
    //
    // RaidStripeTree, Added: linux v6.7
    //
    // RemapTree, Added: linux v7.0
}

impl PartialEq<u64> for TreeId
{
    fn eq(&self, other: &u64) -> bool
    {
        (*self as u64).eq(other)
    }
}

impl PartialOrd<u64> for TreeId
{
    fn partial_cmp(&self, other: &u64) -> Option<std::cmp::Ordering>
    {
        (*self as u64).partial_cmp(other)
    }
}

impl PartialEq<TreeId> for u64
{
    fn eq(&self, other: &TreeId) -> bool
    {
        (*other as u64).eq(self)
    }
}

impl PartialOrd<TreeId> for u64
{
    fn partial_cmp(&self, other: &TreeId) -> Option<std::cmp::Ordering>
    {
        (*other as u64).partial_cmp(self)
    }
}

/// private module to get a `btrfs_ioctl_search_key`
mod seal
{
    pub trait SearchKeyBuilderExt
    {
        fn get_key(&mut self) -> &mut super::btrfs_ioctl_search_key;
    }
}

/// This trait provides methods used to set and update the search key used for a Btrfs Tree Search.
///
/// Search key fields that are used to set minimum and maximum bounds for a Tree Search can be set
/// using the Rust [`std::ops::RangeBounds`] syntax, where the `start_bound` will set the lower
/// bounds and `end_bound` will set the higher bound. All ranges are treated as inclusive, and
/// unbounded ranges are ignored. The special `..=` syntax can be used to set both the minimum
/// and maximum bounds to the same things.
///
/// # Example
///
/// The following example shows how the minimum and maximum bounds can be set for the `objectid`
/// key field.
///
/// ```no_run,rustfmt::skip
/// use libbtrfs::tree_search::{SearchBuilder, SearchKeyBuilder, TreeId};
///
/// SearchBuilder::from_path("/")?
///     // search the root tree
///     .tree(TreeId::RootTree)
///
///     // return at most 20 items
///     .item_limit(20)
///
///     // set the minimum objectid to 256 and the maximum objectid to u64::MAX
///     .objectid(256..u64::MAX)
///
///     // as above
///     .objectid(256..=u64::MAX)
///
///     // sets the minimum objectid to 500 and the maximum objectid is left unchanged
///     .objectid(500..)
///
///     // sets the maximum objectid to 2000 and the minimum is left unchanged
///     .objectid(..2000)
///
///     // sets BOTH minimum and maximum objectid to 1000
///     .objectid(..=1000)
///
///     // consume the builder and return a TreeSearch
///     .build();
///
/// Ok::<(), std::io::Error>(())
/// ````
pub trait SearchKeyBuilder: seal::SearchKeyBuilderExt + Sized
{
    /// Set the tree to be searched.
    ///
    /// Default is `TreeId::RootTree`
    fn tree(mut self, tree_id: TreeId) -> Self
    {
        self.get_key().tree_id = tree_id as u64;
        self
    }

    /// Limit the number of items that the search will find.
    ///
    /// Default is `u32::MAX`
    fn item_limit(mut self, limit: u32) -> Self
    {
        self.get_key().nr_items = limit;
        self
    }

    /// Set the minimum and maximum offset bounds.
    ///
    /// Default is `u64::MIN..u64::MAX`
    fn offset(mut self, offset: impl RangeBounds<u64>) -> Self
    {
        tree_search_set_key_by_range!(offset; self.get_key -> min_offset | max_offset)
    }

    /// Set the minimum and maximum objectid bounds.
    ///
    /// Default is `u64::MIN..u64::MAX`
    fn objectid(mut self, objectid: impl RangeBounds<u64>) -> Self
    {
        tree_search_set_key_by_range!(objectid; self.get_key -> min_objectid | max_objectid)
    }

    /// Set the minimum and maximum transid bounds.
    ///
    /// Default is `u64::MIN..u64::MAX`
    fn transid(mut self, transid: impl RangeBounds<u64>) -> Self
    {
        tree_search_set_key_by_range!(transid; self.get_key -> min_transid | max_transid)
    }

    /// Set the minimum and maximum type bounds.
    ///
    /// Default is `u32::MIN..u32::MAX`
    fn item_type(mut self, item_type: impl RangeBounds<u32>) -> Self
    {
        tree_search_set_key_by_range!(item_type; self.get_key -> min_type | max_type)
    }
}
