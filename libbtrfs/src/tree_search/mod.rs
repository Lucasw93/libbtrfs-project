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
    util::btrfs_ioctl,
};
use std::{
    alloc::{Layout, alloc, dealloc, handle_alloc_error},
    fs::File,
    io,
    marker::PhantomData,
    mem::{ManuallyDrop, MaybeUninit},
    ops::{Bound, RangeBounds},
    os::fd::AsFd,
    path::Path,
    ptr::{read_unaligned, write},
};
pub mod tree_item;

mod builder;
mod query;

use tree_item::TreeItem;

pub use builder::{SearchBuilder, SearchKeyBuilder};
pub use query::{
    FromQueryKey, ObjectId, Offset, Query, QueryKey, Ty, next_objectid, next_offset, next_type,
};

/// Represents a tree in the BTRFS filesystem.
///
/// Argument for [`SearchKeyBuilder::tree()`], which will determine what tree will be searched by
/// the [`query()`] method
///
/// Note: documenation for fields has been taken verbatum from the Linux Kernel at, `btrfs_tree.h`
///
/// [`query()`]: Query::query
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

    /// Tracks RAID stripes in block groups.
    RaidStripeTree = crate::bindings::BTRFS_RAID_STRIPE_TREE_OBJECTID,

    /// Holds details of remapped addresses after relocation.
    RemapTree = crate::bindings::BTRFS_REMAP_TREE_OBJECTID,
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

// ============================================================================
// Iterater. returned by the search method for both stack and heap searches

/// Container for the function called on the `Drop` method of `Iter`
struct FnOnDrop<'buf, F, K>
where
    K: FromQueryKey,
    F: FnOnce(QueryKey) -> K,
{
    f: ManuallyDrop<F>,
    key: &'buf mut btrfs_ioctl_search_key,
}

/// Iterator over items of a btrfs tree search
struct Iter<'buf, F, K>
where
    K: FromQueryKey,
    F: FnOnce(QueryKey) -> K,
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
    K: FromQueryKey,
    F: FnOnce(QueryKey) -> K,
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

        if let Some((obj, ty, off)) = callback((hdr.objectid, hdr.type_, hdr.offset)).as_query_key()
        {
            self.on_drop.key.min_objectid = obj;
            self.on_drop.key.min_type = ty;
            self.on_drop.key.min_offset = off;
        }
    }
}

impl<'buf, F, K> ExactSizeIterator for Iter<'buf, F, K>
where
    K: FromQueryKey,
    F: FnOnce(QueryKey) -> K,
{
}

impl<'buf, F, K> Iterator for Iter<'buf, F, K>
where
    K: FromQueryKey,
    F: FnOnce(QueryKey) -> K,
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

/// Items returned from a btrfs tree search.
///
/// This struct is yeilded from call's to [`next()`], via the iterator constructed by the
/// [`query()`] method.
///
/// [`query()`]: Query::query
/// [`next()`]: Iterator::next
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

    /// Return the sorting key for this item
    pub const fn key(self) -> QueryKey
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

// ======================================================================================
// TreeSearch and BoxedTreeSearch structs. Both Implementors of Query. TreeSearch is inteded
// to live in the stack and has a fixed size buffer of 3992 bytes wich makes the struct 4k in total.
// The BoxedTreeSearch buffer is a dynamically sized array.
//
// Both TreeSearch and BoxedTreeSearch implment the SearchKeyBuilder trait but as mutable
// references so they can update their search key without consuming the struct.

/// Can perform tree searches using a fixed size buffer.
///
/// This struct is capable of performing tree searches via the [`Query`] trait.
///
/// This struct is returned by the [`SearchBuilder::build()`] method.
///
/// See the [`SearchKeyBuilder`] trait for updating the search key.
pub struct TreeSearch<R: AsFd>
{
    resource: R,
    args: MaybeUninit<btrfs_ioctl_search_args>,
}

impl<'a, R: AsFd> builder::seal::SearchKeyBuilderExt for &'a mut TreeSearch<R>
{
    #[inline(always)]
    fn get_key(&mut self) -> &mut self::btrfs_ioctl_search_key
    {
        unsafe { &mut *(&raw mut (*self.args.as_mut_ptr()).key) }
    }
}

impl<'a, R: AsFd> SearchKeyBuilder for &'a mut TreeSearch<R> {}

impl<R: AsFd> Query for TreeSearch<R>
{
    fn query<'buf, F, K>(
        &'buf mut self,
        on_drop: F,
    ) -> io::Result<impl Iterator<Item = SearchItem<'buf>> + ExactSizeIterator>
    where
        K: FromQueryKey,
        F: FnOnce(QueryKey) -> K,
    {
        let fd = self.resource.as_fd();
        let args = self.args.as_mut_ptr();
        let key = unsafe { &mut *(&raw mut (*args).key) };
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

/// Can perform tree searches using a variable size buffer.
///
/// This struct is capable of performing tree searches via the [`Query`] trait.
///
/// This struct is returned by the [`SearchBuilder::build_boxed()`] method.
///
/// This struct is equivelent to the [`TreeSearch`] struct, except that the search buffer is
/// allocated on the heap. The buffer size is determied by the argument given to the
/// [`SearchBuilder::build_boxed()`] method.
///
/// See the [`SearchKeyBuilder`] trait for updating the search key.
pub struct BoxedTreeSearch<R: AsFd>
{
    resource: R,
    args: *mut btrfs_ioctl_search_args_v2,
}

impl<'a, R: AsFd> builder::seal::SearchKeyBuilderExt for &'a mut BoxedTreeSearch<R>
{
    #[inline(always)]
    fn get_key(&mut self) -> &mut self::btrfs_ioctl_search_key
    {
        unsafe { &mut *(&raw mut (*self.args).key) }
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
    ) -> io::Result<impl Iterator<Item = SearchItem<'buf>> + ExactSizeIterator>
    where
        K: FromQueryKey,
        F: FnOnce(QueryKey) -> K,
    {
        let fd = self.resource.as_fd();
        let args = self.args;
        let key = unsafe { &mut *(&raw mut (*args).key) };
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
