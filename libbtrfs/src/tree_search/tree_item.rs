#![allow(missing_docs)]
//! Items found by a BTRFS Tree Search
//!
//! For documentation of BTRFS Tree items, please see, [btrfs-dev-docs - tree-items.txt](https://github.com/btrfs/btrfs-dev-docs/blob/master/tree-items.txt).
use super::*;

use crate::{
    bindings::{
        btrfs_balance_item, btrfs_block_group_item, btrfs_chunk, btrfs_csum_item,
        btrfs_dev_extent, btrfs_dev_item, btrfs_dev_replace_item, btrfs_dev_stats_item,
        btrfs_dir_item, btrfs_dir_log_item, btrfs_disk_balance_args, btrfs_disk_key,
        btrfs_extent_data_ref, btrfs_extent_inline_ref, btrfs_extent_item, btrfs_file_extent_item,
        btrfs_free_space_header, btrfs_free_space_info, btrfs_inode_extref, btrfs_inode_item,
        btrfs_inode_ref, btrfs_ioctl_timespec, btrfs_qgroup_info_item, btrfs_qgroup_limit_item,
        btrfs_qgroup_status_item, btrfs_root_item, btrfs_root_ref, btrfs_shared_data_ref,
        btrfs_stripe, btrfs_timespec, BTRFS_BALANCE_ARGS_LIMIT, BTRFS_BALANCE_ARGS_LIMIT_RANGE,
        BTRFS_BALANCE_ARGS_USAGE, BTRFS_BALANCE_ARGS_USAGE_RANGE, BTRFS_BLOCK_GROUP_ITEM_KEY,
        BTRFS_CHUNK_ITEM_KEY, BTRFS_DEV_EXTENT_KEY, BTRFS_DEV_ITEM_KEY, BTRFS_DEV_REPLACE_KEY,
        BTRFS_DEV_STAT_VALUES_MAX, BTRFS_DIR_INDEX_KEY, BTRFS_DIR_ITEM_KEY,
        BTRFS_DIR_LOG_INDEX_KEY, BTRFS_DIR_LOG_ITEM_KEY, BTRFS_EXTENT_CSUM_KEY,
        BTRFS_EXTENT_DATA_KEY, BTRFS_EXTENT_ITEM_KEY, BTRFS_FREE_SPACE_BITMAP_KEY,
        BTRFS_FREE_SPACE_EXTENT_KEY, BTRFS_FREE_SPACE_INFO_KEY, BTRFS_INODE_EXTREF_KEY,
        BTRFS_INODE_ITEM_KEY, BTRFS_INODE_REF_KEY, BTRFS_METADATA_ITEM_KEY, BTRFS_ORPHAN_ITEM_KEY,
        BTRFS_PERSISTENT_ITEM_KEY, BTRFS_QGROUP_INFO_KEY, BTRFS_QGROUP_LIMIT_KEY,
        BTRFS_QGROUP_RELATION_KEY, BTRFS_QGROUP_STATUS_KEY, BTRFS_ROOT_BACKREF_KEY,
        BTRFS_ROOT_ITEM_KEY, BTRFS_ROOT_REF_KEY, BTRFS_TEMPORARY_ITEM_KEY,
        BTRFS_UUID_KEY_RECEIVED_SUBVOL, BTRFS_UUID_KEY_SUBVOL, BTRFS_XATTR_ITEM_KEY,
    },
    util::{IoResult, KernelStr},
};
use uuid::Uuid;

pub trait TreeItem<'buf>
{
    const KEY: u32;

    unsafe fn from_search_unckeched<T>(p: *const T) -> Self;

    #[inline]
    fn from_search<T>(p: *const T, key: u32) -> Option<Self>
    where
        Self: Sized,
    {
        (key == Self::KEY).then_some(unsafe { Self::from_search_unckeched(p) })
    }
}

mod seal
{
    pub trait ItemNameMeta<'buf>: super::TreeItem<'buf>
    {
        fn name_len(&self) -> usize;

        fn name_ptr(&self) -> *const u8;
    }
}

impl<'buf, T: seal::ItemNameMeta<'buf>> TreeItemName<'buf> for T {}

pub trait TreeItemName<'buf>: seal::ItemNameMeta<'buf>
{
    fn name_bytes(&self) -> IoResult<&[u8]>
    {
        let bytes = unsafe { std::slice::from_raw_parts(self.name_ptr(), self.name_len()) };

        if bytes.contains(&0) {
            Err(std::io::ErrorKind::InvalidData.into())
        } else {
            Ok(bytes)
        }
    }

    fn name_str(&self) -> IoResult<KernelStr<'_>>
    {
        self.name_bytes().map(String::from_utf8_lossy)
    }
}

macro_rules! tree_item_get {
    (__u8($__self:ident -> $field:ident)) => {
        unsafe {
            let fieldp = &raw const (*$__self.0).$field;
            // for type saftey
            u8::from_le(::std::ptr::read_unaligned(fieldp))
        }
    };
    (__le16( $__self:ident -> $field:ident )) => {
        unsafe {
            let fieldp = &raw const (*$__self.0).$field;

            u16::from_le(::std::ptr::read_unaligned(fieldp))
        }
    };
    (__le32( $__self:ident -> $field:ident )) => {
        unsafe {
            let fieldp = &raw const (*$__self.0).$field;

            u32::from_le(::std::ptr::read_unaligned(fieldp))
        }
    };
    (__le64( $__self:ident -> $field:ident )) => {
        unsafe {
            let fieldp = &raw const (*$__self.0).$field;

            u64::from_le(::std::ptr::read_unaligned(fieldp))
        }
    };
    (::Uuid( $__self:ident -> $field:ident )) => {
        unsafe {
            let fieldp = &raw const (*$__self.0).$field;

            ::uuid::Uuid::from_bytes(::std::ptr::read_unaligned(fieldp))
        }
    };
}

pub struct Timespec<'a>(*const btrfs_timespec, PhantomData<&'a c_char>);

impl From<Timespec<'_>> for btrfs_ioctl_timespec
{
    fn from(value: Timespec<'_>) -> Self
    {
        unsafe {
            let times = (&raw const value.0).read_unaligned();
            Self {
                sec: u64::from_le((*times).sec),
                nsec: u32::from_le((*times).nsec),
            }
        }
    }
}

impl From<Timespec<'_>> for libc::timespec
{
    fn from(value: Timespec<'_>) -> Self
    {
        unsafe {
            let times = (&raw const value.0).read_unaligned();
            Self {
                tv_nsec: i64::from_le((*times).sec as i64),
                tv_sec: i64::from_le((*times).nsec as i64),
            }
        }
    }
}

impl Timespec<'_>
{
    pub const fn nsec(&self) -> u32
    {
        tree_item_get!(__le32(self->nsec))
    }

    pub const fn sec(&self) -> u64
    {
        tree_item_get!(__le64(self->sec))
    }
}

pub struct DiskKey<'a>(*const btrfs_disk_key, PhantomData<&'a c_char>);

impl DiskKey<'_>
{
    pub const fn objectid(&self) -> u64
    {
        tree_item_get!(__le64(self->objectid))
    }

    pub const fn offset(&self) -> u64
    {
        tree_item_get!(__le64(self->offset))
    }

    pub const fn type_(&self) -> u8
    {
        tree_item_get!(__u8(self->type_))
    }
}

// =========================================================================
// ROOT item

pub struct RootItem<'a>(*const btrfs_root_item, PhantomData<&'a c_char>);

impl<'buf> TreeItem<'buf> for RootItem<'buf>
{
    const KEY: u32 = BTRFS_ROOT_ITEM_KEY;

    #[inline]
    unsafe fn from_search_unckeched<T>(bufp: *const T) -> Self
    {
        Self(bufp.cast(), PhantomData)
    }
}

impl RootItem<'_>
{
    pub const fn inode_item(&self) -> InodeItem<'_>
    {
        unsafe { InodeItem((&raw const (*self.0).inode), PhantomData) }
    }

    pub const fn generation(&self) -> u64
    {
        tree_item_get!(__le64(self->generation))
    }

    pub const fn root_dirid(&self) -> u64
    {
        tree_item_get!(__le64(self->root_dirid))
    }

    pub const fn bytenr(&self) -> u64
    {
        tree_item_get!(__le64(self->bytenr))
    }

    pub const fn byte_limit(&self) -> u64
    {
        tree_item_get!(__le64(self->byte_limit))
    }

    pub const fn bytes_used(&self) -> u64
    {
        tree_item_get!(__le64(self->bytes_used))
    }

    pub const fn last_snapshot(&self) -> u64
    {
        tree_item_get!(__le64(self->last_snapshot))
    }

    pub const fn flags(&self) -> u64
    {
        tree_item_get!(__le64(self->flags))
    }

    pub const fn refs(&self) -> u32
    {
        tree_item_get!(__le32(self->refs))
    }

    pub const fn drop_progress(&self) -> DiskKey<'_>
    {
        unsafe { DiskKey((&raw const (*self.0).drop_progress), PhantomData) }
    }

    pub const fn drop_level(&self) -> u8
    {
        tree_item_get!(__u8(self->drop_level))
    }

    pub const fn generation_v2(&self) -> u64
    {
        tree_item_get!(__le64(self->generation_v2))
    }

    pub const fn uuid(&self) -> Uuid
    {
        tree_item_get!(::Uuid(self->uuid))
    }

    pub const fn parent_uuid(&self) -> Uuid
    {
        tree_item_get!(::Uuid(self->parent_uuid))
    }

    pub const fn received_uuid(&self) -> Uuid
    {
        tree_item_get!(::Uuid(self->received_uuid))
    }

    pub const fn otransid(&self) -> u64
    {
        tree_item_get!(__le64(self->otransid))
    }

    pub const fn ctransid(&self) -> u64
    {
        tree_item_get!(__le64(self->ctransid))
    }

    pub const fn stransid(&self) -> u64
    {
        tree_item_get!(__le64(self->stransid))
    }

    pub const fn rtransid(&self) -> u64
    {
        tree_item_get!(__le64(self->rtransid))
    }

    pub const fn ctime(&self) -> Timespec<'_>
    {
        unsafe { Timespec((&raw const (*self.0).ctime), PhantomData) }
    }

    pub const fn otime(&self) -> Timespec<'_>
    {
        unsafe { Timespec((&raw const (*self.0).otime), PhantomData) }
    }

    pub const fn stime(&self) -> Timespec<'_>
    {
        unsafe { Timespec((&raw const (*self.0).stime), PhantomData) }
    }

    pub const fn rtime(&self) -> Timespec<'_>
    {
        unsafe { Timespec((&raw const (*self.0).rtime), PhantomData) }
    }
}

// =========================================================================
// ROOT REF item

pub struct RootRef<'buf>(*const btrfs_root_ref, PhantomData<&'buf c_char>);
pub struct RootBackref<'buf>(*const btrfs_root_ref, PhantomData<&'buf c_char>);

macro_rules! impl_for_btrfs_root_ref {
    (<$buf:lifetime> $btrfs_root_ref:ty => $KEY:expr) => {
        impl<$buf> TreeItem<$buf> for $btrfs_root_ref
        {
            const KEY: u32 = $KEY;

            #[inline]
            unsafe fn from_search_unckeched<T>(bufp: *const T) -> Self
            {
                Self(bufp.cast(), PhantomData)
            }
        }

        impl<$buf> seal::ItemNameMeta<$buf> for $btrfs_root_ref
        {
            #[inline(always)]
            fn name_len(&self) -> usize
            {
                tree_item_get!(__le16(self->name_len)) as usize
            }

            #[inline(always)]
            fn name_ptr(&self) -> *const u8
            {
                unsafe { self.0.add(1).cast() }
            }
        }

        impl<$buf> $btrfs_root_ref
        {
            #[inline(always)]
            pub const fn dirid(&self) -> u64
            {
                tree_item_get!(__le64(self->dirid))
            }

            #[inline(always)]
            pub const fn sequence(&self) -> u64
            {
                tree_item_get!(__le64(self->sequence))
            }
        }
    }
}
impl_for_btrfs_root_ref!(<'buf> RootRef<'buf> => BTRFS_ROOT_REF_KEY);
impl_for_btrfs_root_ref!(<'buf> RootBackref<'buf> => BTRFS_ROOT_BACKREF_KEY);

// =========================================================================
// INODE item

pub struct InodeItem<'a>(*const btrfs_inode_item, PhantomData<&'a c_char>);

impl<'buf> TreeItem<'buf> for InodeItem<'buf>
{
    const KEY: u32 = BTRFS_INODE_ITEM_KEY;

    #[inline]
    unsafe fn from_search_unckeched<T>(bufp: *const T) -> Self
    {
        Self(bufp.cast(), PhantomData)
    }
}

impl InodeItem<'_>
{
    pub const fn generation(&self) -> u64
    {
        tree_item_get!(__le64(self->generation))
    }

    pub const fn transid(&self) -> u64
    {
        tree_item_get!(__le64(self->transid))
    }

    pub const fn size(&self) -> u64
    {
        tree_item_get!(__le64(self->size))
    }

    pub const fn nbytes(&self) -> u64
    {
        tree_item_get!(__le64(self->nbytes))
    }

    pub const fn block_group(&self) -> u64
    {
        tree_item_get!(__le64(self->block_group))
    }

    pub const fn nlink(&self) -> u32
    {
        tree_item_get!(__le32(self->nlink))
    }

    pub const fn uid(&self) -> u32
    {
        tree_item_get!(__le32(self->uid))
    }

    pub const fn gid(&self) -> u32
    {
        tree_item_get!(__le32(self->gid))
    }

    pub const fn mode(&self) -> u32
    {
        tree_item_get!(__le32(self->mode))
    }

    pub const fn rdev(&self) -> u64
    {
        tree_item_get!(__le64(self->rdev))
    }

    pub const fn flags(&self) -> u64
    {
        tree_item_get!(__le64(self->flags))
    }

    pub const fn sequence(&self) -> u64
    {
        tree_item_get!(__le64(self->sequence))
    }

    pub const fn atime(&self) -> Timespec<'_>
    {
        unsafe { Timespec((&raw const (*self.0).atime), PhantomData) }
    }

    pub const fn ctime(&self) -> Timespec<'_>
    {
        unsafe { Timespec((&raw const (*self.0).ctime), PhantomData) }
    }

    pub const fn mtime(&self) -> Timespec<'_>
    {
        unsafe { Timespec((&raw const (*self.0).mtime), PhantomData) }
    }

    pub const fn otime(&self) -> Timespec<'_>
    {
        unsafe { Timespec((&raw const (*self.0).otime), PhantomData) }
    }
}

// =========================================================================
// CHUNK item

pub struct ChunkItem<'a>(*const btrfs_chunk, PhantomData<&'a c_char>);

impl<'buf> TreeItem<'buf> for ChunkItem<'buf>
{
    const KEY: u32 = BTRFS_CHUNK_ITEM_KEY;

    #[inline]
    unsafe fn from_search_unckeched<T>(bufp: *const T) -> Self
    {
        Self(bufp.cast(), PhantomData)
    }
}

impl ChunkItem<'_>
{
    pub const fn length(&self) -> u64
    {
        tree_item_get!(__le64(self->length))
    }
    pub const fn owner(&self) -> u64
    {
        tree_item_get!(__le64(self->owner))
    }

    pub const fn stripe_len(&self) -> u64
    {
        tree_item_get!(__le64(self->stripe_len))
    }

    pub const fn type_(&self) -> u64
    {
        tree_item_get!(__le64(self->type_))
    }

    pub const fn io_align(&self) -> u32
    {
        tree_item_get!(__le32(self->io_align))
    }

    pub const fn io_width(&self) -> u32
    {
        tree_item_get!(__le32(self->io_width))
    }

    pub const fn sector_size(&self) -> u32
    {
        tree_item_get!(__le32(self->sector_size))
    }

    pub const fn num_stripes(&self) -> u16
    {
        tree_item_get!(__le16(self->num_stripes))
    }

    pub const fn sub_stripes(&self) -> u64
    {
        tree_item_get!(__le64(self->stripe_len))
    }

    pub const fn stripts(&self) -> Stripe<'_>
    {
        unsafe { Stripe((&raw const (*self.0).stripe), PhantomData) }
    }
}

pub struct Stripe<'a>(*const btrfs_stripe, PhantomData<&'a c_char>);

impl Stripe<'_>
{
    pub const fn devid(&self) -> u64
    {
        tree_item_get!(__le64(self->devid))
    }

    pub const fn offset(&self) -> u64
    {
        tree_item_get!(__le64(self->offset))
    }

    pub const fn dev_uuid(&self) -> Uuid
    {
        tree_item_get!(::Uuid(self->dev_uuid))
    }
}

// =========================================================================
// DEVICE item

pub struct DevItem<'a>(*const btrfs_dev_item, PhantomData<&'a c_char>);

impl<'buf> TreeItem<'buf> for DevItem<'buf>
{
    const KEY: u32 = BTRFS_DEV_ITEM_KEY;

    #[inline]
    unsafe fn from_search_unckeched<T>(bufp: *const T) -> Self
    {
        Self(bufp.cast(), PhantomData)
    }
}

impl DevItem<'_>
{
    pub const fn devid(&self) -> u64
    {
        tree_item_get!(__le64(self->devid))
    }

    pub const fn total_bytes(&self) -> u64
    {
        tree_item_get!(__le64(self->total_bytes))
    }

    pub const fn bytes_used(&self) -> u64
    {
        tree_item_get!(__le64(self->bytes_used))
    }

    pub const fn io_align(&self) -> u32
    {
        tree_item_get!(__le32(self->io_align))
    }

    pub const fn io_width(&self) -> u32
    {
        tree_item_get!(__le32(self->io_width))
    }

    pub const fn sector_size(&self) -> u32
    {
        tree_item_get!(__le32(self->sector_size))
    }

    pub const fn uuid(&self) -> Uuid
    {
        tree_item_get!(::Uuid(self->uuid))
    }

    pub const fn fsid(&self) -> Uuid
    {
        tree_item_get!(::Uuid(self->fsid))
    }
}

// =========================================================================
// DEVICE EXTENT item

pub struct DevExtent<'a>(*const btrfs_dev_extent, PhantomData<&'a c_char>);

impl<'buf> TreeItem<'buf> for DevExtent<'buf>
{
    const KEY: u32 = BTRFS_DEV_EXTENT_KEY;

    #[inline]
    unsafe fn from_search_unckeched<T>(bufp: *const T) -> Self
    {
        Self(bufp.cast(), PhantomData)
    }
}

impl DevExtent<'_>
{
    pub const fn chunk_tree(&self) -> u64
    {
        tree_item_get!(__le64(self->chunk_tree))
    }

    pub const fn chunk_objectid(&self) -> u64
    {
        tree_item_get!(__le64(self->chunk_objectid))
    }

    pub const fn chunk_offset(&self) -> u64
    {
        tree_item_get!(__le64(self->chunk_offset))
    }

    pub const fn length(&self) -> u64
    {
        tree_item_get!(__le64(self->length))
    }

    pub const fn chunk_tree_uuid(&self) -> Uuid
    {
        tree_item_get!(::Uuid(self->chunk_tree_uuid))
    }
}

// =========================================================================
// DEVICE STATS item

pub struct DevStatsItem<'a>(*const btrfs_dev_stats_item, PhantomData<&'a c_char>);

impl<'buf> TreeItem<'buf> for DevStatsItem<'buf>
{
    const KEY: u32 = BTRFS_PERSISTENT_ITEM_KEY;

    #[inline]
    unsafe fn from_search_unckeched<T>(bufp: *const T) -> Self
    {
        Self(bufp.cast(), PhantomData)
    }
}

impl DevStatsItem<'_>
{
    pub fn values(&self) -> [u64; BTRFS_DEV_STAT_VALUES_MAX as usize]
    {
        unsafe { (&raw const (*self.0).values).read_unaligned() }.map(|v| u64::from_le(v))
    }
}

// =========================================================================
// DEVICE REPLACE item

pub struct DevReplaceItem<'a>(*const btrfs_dev_replace_item, PhantomData<&'a c_char>);

impl<'buf> TreeItem<'buf> for DevReplaceItem<'buf>
{
    const KEY: u32 = BTRFS_DEV_REPLACE_KEY;

    #[inline]
    unsafe fn from_search_unckeched<T>(bufp: *const T) -> Self
    {
        Self(bufp.cast(), PhantomData)
    }
}

impl DevReplaceItem<'_>
{
    pub const fn src_devid(&self) -> u64
    {
        tree_item_get!(__le64(self->src_devid))
    }

    pub const fn cursor_left(&self) -> u64
    {
        tree_item_get!(__le64(self->cursor_left))
    }

    pub const fn cursor_right(&self) -> u64
    {
        tree_item_get!(__le64(self->cursor_right))
    }

    pub const fn cont_reading_from_srcdev_mode(&self) -> u64
    {
        tree_item_get!(__le64(self->cont_reading_from_srcdev_mode))
    }

    pub const fn time_started(&self) -> u64
    {
        tree_item_get!(__le64(self->time_started))
    }

    pub const fn time_stopped(&self) -> u64
    {
        tree_item_get!(__le64(self->time_stopped))
    }

    pub const fn num_write_errors(&self) -> u64
    {
        tree_item_get!(__le64(self->num_write_errors))
    }

    pub const fn num_uncorrectable_read_errors(&self) -> u64
    {
        tree_item_get!(__le64(self->num_uncorrectable_read_errors))
    }
}

// =========================================================================
// BLOCK GROUP item

pub struct BlockGroupItem<'a>(*const btrfs_block_group_item, PhantomData<&'a c_char>);

impl<'buf> TreeItem<'buf> for BlockGroupItem<'buf>
{
    const KEY: u32 = BTRFS_BLOCK_GROUP_ITEM_KEY;

    #[inline]
    unsafe fn from_search_unckeched<T>(bufp: *const T) -> Self
    {
        Self(bufp.cast(), PhantomData)
    }
}

impl BlockGroupItem<'_>
{
    pub const fn used(&self) -> u64
    {
        tree_item_get!(__le64(self->used))
    }

    pub const fn chunk_objectid(&self) -> u64
    {
        tree_item_get!(__le64(self->chunk_objectid))
    }

    pub const fn flags(&self) -> u64
    {
        tree_item_get!(__le64(self->flags))
    }
}

// =========================================================================
// FILE EXTENT DATA item

pub struct FileExtentItem<'a>(*const btrfs_file_extent_item, PhantomData<&'a c_char>);

impl<'buf> TreeItem<'buf> for FileExtentItem<'buf>
{
    const KEY: u32 = BTRFS_EXTENT_DATA_KEY;

    #[inline]
    unsafe fn from_search_unckeched<T>(bufp: *const T) -> Self
    {
        Self(bufp.cast(), PhantomData)
    }
}

impl FileExtentItem<'_>
{
    pub const fn generation(&self) -> u64
    {
        tree_item_get!(__le64(self->generation))
    }

    pub const fn ram_bytes(&self) -> u64
    {
        tree_item_get!(__le64(self->ram_bytes))
    }

    pub const fn compression(&self) -> u8
    {
        tree_item_get!(__u8(self->compression))
    }

    pub const fn encryption(&self) -> u8
    {
        tree_item_get!(__u8(self->encryption))
    }

    pub const fn other_encoding(&self) -> u16
    {
        tree_item_get!(__le16(self->other_encoding))
    }

    pub const fn type_(&self) -> u8
    {
        tree_item_get!(__u8(self->type_))
    }

    pub const fn disk_bytenr(&self) -> u64
    {
        tree_item_get!(__le64(self->disk_bytenr))
    }

    pub const fn disk_num_bytes(&self) -> u64
    {
        tree_item_get!(__le64(self->disk_num_bytes))
    }

    pub const fn offset(&self) -> u64
    {
        tree_item_get!(__le64(self->offset))
    }

    pub const fn num_bytes(&self) -> u64
    {
        tree_item_get!(__le64(self->num_bytes))
    }
}

// =========================================================================
// EXTENT item

pub struct ExtentItem<'a>(*const btrfs_extent_item, PhantomData<&'a c_char>);

impl<'buf> TreeItem<'buf> for ExtentItem<'buf>
{
    const KEY: u32 = BTRFS_EXTENT_ITEM_KEY;

    #[inline]
    unsafe fn from_search_unckeched<T>(bufp: *const T) -> Self
    {
        Self(bufp.cast(), PhantomData)
    }
}

impl ExtentItem<'_>
{
    pub const fn refs(&self) -> u64
    {
        tree_item_get!(__le64(self->refs))
    }

    pub const fn generation(&self) -> u64
    {
        tree_item_get!(__le64(self->generation))
    }

    pub const fn flags(&self) -> u64
    {
        tree_item_get!(__le64(self->flags))
    }
}

pub struct ExtentDataRef<'a>(*const btrfs_extent_data_ref, PhantomData<&'a c_char>);

impl ExtentDataRef<'_>
{
    pub const fn root(&self) -> u64
    {
        tree_item_get!(__le64(self->root))
    }

    pub const fn objectid(&self) -> u64
    {
        tree_item_get!(__le64(self->objectid))
    }

    pub const fn offset(&self) -> u64
    {
        tree_item_get!(__le64(self->offset))
    }

    pub const fn count(&self) -> u32
    {
        tree_item_get!(__le32(self->count))
    }
}

pub struct SharedDataRef<'a>(*const btrfs_shared_data_ref, PhantomData<&'a c_char>);

impl SharedDataRef<'_>
{
    pub const fn count(&self) -> u32
    {
        tree_item_get!(__le32(self->count))
    }
}

// =========================================================================
// METADATA EXTENT item

pub struct ExtentInlineRef<'a>(*const btrfs_extent_inline_ref, PhantomData<&'a c_char>);

impl<'buf> TreeItem<'buf> for ExtentInlineRef<'buf>
{
    const KEY: u32 = BTRFS_METADATA_ITEM_KEY;

    #[inline]
    unsafe fn from_search_unckeched<T>(bufp: *const T) -> Self
    {
        Self(bufp.cast(), PhantomData)
    }
}

impl ExtentInlineRef<'_>
{
    pub const fn type_(&self) -> u8
    {
        tree_item_get!(__u8(self->type_))
    }

    pub const fn offset(&self) -> u64
    {
        tree_item_get!(__le64(self->offset))
    }
}

// =========================================================================
// CHECKSUM item

pub struct ExtentCsum<'a>(*const btrfs_csum_item, PhantomData<&'a c_char>);

impl<'buf> TreeItem<'buf> for ExtentCsum<'buf>
{
    const KEY: u32 = BTRFS_EXTENT_CSUM_KEY;

    #[inline]
    unsafe fn from_search_unckeched<T>(bufp: *const T) -> Self
    {
        Self(bufp.cast(), PhantomData)
    }
}

impl ExtentCsum<'_>
{
    pub const fn csum(&self) -> u8
    {
        tree_item_get!(__u8(self->csum))
    }
}

// =========================================================================
// FREE SPACE INFO item (v2 cache)

pub struct FreeSpaceInfo<'a>(*const btrfs_free_space_info, PhantomData<&'a c_char>);

impl<'buf> TreeItem<'buf> for FreeSpaceInfo<'buf>
{
    const KEY: u32 = BTRFS_FREE_SPACE_INFO_KEY;

    #[inline]
    unsafe fn from_search_unckeched<T>(bufp: *const T) -> Self
    {
        Self(bufp.cast(), PhantomData)
    }
}

impl FreeSpaceInfo<'_>
{
    pub const fn extent_count(&self) -> u32
    {
        tree_item_get!(__le32(self->extent_count))
    }

    pub const fn flags(&self) -> u32
    {
        tree_item_get!(__le32(self->flags))
    }
}

// =========================================================================
// FREE SPACE EXTENT item (v2 cache)

pub struct FreeSpaceExtent<'a>(PhantomData<&'a c_char>);

impl<'buf> TreeItem<'buf> for FreeSpaceExtent<'buf>
{
    const KEY: u32 = BTRFS_FREE_SPACE_EXTENT_KEY;

    #[inline]
    unsafe fn from_search_unckeched<T>(_bufp: *const T) -> Self
    {
        Self(PhantomData)
    }
}

// =========================================================================
// FREE SPACE BITMAP item (v2 cache)

pub struct FreeSpaceBitmap<'a>(PhantomData<&'a c_char>);

impl<'buf> TreeItem<'buf> for FreeSpaceBitmap<'buf>
{
    const KEY: u32 = BTRFS_FREE_SPACE_BITMAP_KEY;

    #[inline]
    unsafe fn from_search_unckeched<T>(_bufp: *const T) -> Self
    {
        Self(PhantomData)
    }
}

// =========================================================================
// FREE SPACE HEADER item (v1 cache)

pub struct FreeSpaceHeader<'a>(*const btrfs_free_space_header, PhantomData<&'a c_char>);

impl<'buf> TreeItem<'buf> for FreeSpaceHeader<'buf>
{
    const KEY: u32 = 0;

    #[inline]
    unsafe fn from_search_unckeched<T>(bufp: *const T) -> Self
    {
        Self(bufp.cast(), PhantomData)
    }
}

impl FreeSpaceHeader<'_>
{
    pub const fn location(&self) -> DiskKey<'_>
    {
        unsafe { DiskKey((&raw const (*self.0).location), PhantomData) }
    }

    pub const fn generation(&self) -> u64
    {
        tree_item_get!(__le64(self->generation))
    }

    pub const fn num_entries(&self) -> u64
    {
        tree_item_get!(__le64(self->num_entries))
    }

    pub const fn num_bitmaps(&self) -> u64
    {
        tree_item_get!(__le64(self->num_bitmaps))
    }
}

// =========================================================================
// DIR item / DIR INDEX item

pub struct DirItem<'a>(*const btrfs_dir_item, PhantomData<&'a c_char>);
pub struct DirIndex<'a>(*const btrfs_dir_item, PhantomData<&'a c_char>);
pub struct XattrItem<'a>(*const btrfs_dir_item, PhantomData<&'a c_char>);

macro_rules! impl_for_btrfs_dir_item {
    ( <$buf:lifetime> $btrfs_dir_item:ty => $key:expr ) => {
        impl<$buf> TreeItem<$buf> for $btrfs_dir_item
        {
            const KEY: u32 = $key;

            #[inline(always)] unsafe fn from_search_unckeched<T>(bufp: *const T) -> Self
            {
                Self(bufp.cast(), PhantomData)
            }
        }

        impl<$buf> seal::ItemNameMeta<$buf> for $btrfs_dir_item
        {
            #[inline(always)]
            fn name_len(&self) -> usize
            {
                tree_item_get!(__le16(self->name_len)) as usize
            }

            #[inline(always)]
            fn name_ptr(&self) -> *const u8
            {
                unsafe { self.0.add(1).cast() }
            }
        }

        impl<$buf> $btrfs_dir_item
        {
            #[inline(always)]
            pub fn data_len(&self) -> u16
            {
                tree_item_get!(__le16(self->data_len))
            }

            #[inline(always)]
            pub fn transid(&self) -> u64
            {
                tree_item_get!(__le64(self->transid))
            }

            #[inline(always)]
            pub const fn location(&self) -> DiskKey<'_>
            {
                unsafe { DiskKey((&raw const (*self.0).location), PhantomData) }
            }
        }
    }
}
impl_for_btrfs_dir_item!(<'buf> DirItem<'buf> => BTRFS_DIR_ITEM_KEY);
impl_for_btrfs_dir_item!(<'buf> DirIndex<'buf> => BTRFS_DIR_INDEX_KEY);
impl_for_btrfs_dir_item!(<'buf> XattrItem<'buf> => BTRFS_XATTR_ITEM_KEY);

// =========================================================================
// INODE REF item

pub struct InodeRef<'a>(*const btrfs_inode_ref, PhantomData<&'a c_char>);

impl<'buf> TreeItem<'buf> for InodeRef<'buf>
{
    const KEY: u32 = BTRFS_INODE_REF_KEY;

    #[inline]
    unsafe fn from_search_unckeched<T>(bufp: *const T) -> Self
    {
        Self(bufp.cast(), PhantomData)
    }
}

impl<'buf> seal::ItemNameMeta<'buf> for InodeRef<'buf>
{
    #[inline(always)]
    fn name_len(&self) -> usize
    {
        tree_item_get!(__le16(self->name_len)) as usize
    }

    #[inline(always)]
    fn name_ptr(&self) -> *const u8
    {
        unsafe { self.0.add(1).cast() }
    }
}

impl InodeRef<'_>
{
    pub const fn data_len(&self) -> u64
    {
        tree_item_get!(__le64(self->index))
    }
}

// =========================================================================
// EXTENDED INODE REF item

pub struct InodeExtref<'a>(*const btrfs_inode_extref, PhantomData<&'a c_char>);

impl<'buf> TreeItem<'buf> for InodeExtref<'buf>
{
    const KEY: u32 = BTRFS_INODE_EXTREF_KEY;

    #[inline]
    unsafe fn from_search_unckeched<T>(bufp: *const T) -> Self
    {
        Self(bufp.cast(), PhantomData)
    }
}

impl<'buf> seal::ItemNameMeta<'buf> for InodeExtref<'buf>
{
    #[inline(always)]
    fn name_len(&self) -> usize
    {
        tree_item_get!(__le16(self->name_len)) as usize
    }

    #[inline(always)]
    fn name_ptr(&self) -> *const u8
    {
        unsafe { self.0.add(1).cast() }
    }
}

impl InodeExtref<'_>
{
    pub const fn parent_objectid(&self) -> u64
    {
        tree_item_get!(__le64(self->parent_objectid))
    }

    pub const fn index(&self) -> u64
    {
        tree_item_get!(__le64(self->index))
    }

    pub const fn data_len(&self) -> u64
    {
        tree_item_get!(__le64(self->index))
    }
}

// =========================================================================
// QGROUP STATUS item

pub struct QgroupStatus<'a>(*const btrfs_qgroup_status_item, PhantomData<&'a c_char>);

impl<'buf> TreeItem<'buf> for QgroupStatus<'buf>
{
    const KEY: u32 = BTRFS_QGROUP_STATUS_KEY;

    #[inline]
    unsafe fn from_search_unckeched<T>(bufp: *const T) -> Self
    {
        Self(bufp.cast(), PhantomData)
    }
}

impl QgroupStatus<'_>
{
    pub const fn version(&self) -> u64
    {
        tree_item_get!(__le64(self->version))
    }

    pub const fn generation(&self) -> u64
    {
        tree_item_get!(__le64(self->generation))
    }

    pub const fn flags(&self) -> u64
    {
        tree_item_get!(__le64(self->flags))
    }

    pub const fn rescan(&self) -> u64
    {
        tree_item_get!(__le64(self->rescan))
    }
}

// =========================================================================
// QGROUP INFO item

pub struct QgroupInfo<'a>(*const btrfs_qgroup_info_item, PhantomData<&'a c_char>);

impl<'buf> TreeItem<'buf> for QgroupInfo<'buf>
{
    const KEY: u32 = BTRFS_QGROUP_INFO_KEY;

    #[inline]
    unsafe fn from_search_unckeched<T>(bufp: *const T) -> Self
    {
        Self(bufp.cast(), PhantomData)
    }
}

impl QgroupInfo<'_>
{
    pub const fn generation(&self) -> u64
    {
        tree_item_get!(__le64(self->generation))
    }

    pub const fn rfer(&self) -> u64
    {
        tree_item_get!(__le64(self->rfer))
    }

    pub const fn rfer_cmpr(&self) -> u64
    {
        tree_item_get!(__le64(self->rfer_cmpr))
    }

    pub const fn excl(&self) -> u64
    {
        tree_item_get!(__le64(self->excl))
    }

    pub const fn excl_cmpr(&self) -> u64
    {
        tree_item_get!(__le64(self->excl_cmpr))
    }
}

// =========================================================================
// QGROUP LIMIT item

pub struct QgroupLimit<'a>(*const btrfs_qgroup_limit_item, PhantomData<&'a c_char>);

impl<'buf> TreeItem<'buf> for QgroupLimit<'buf>
{
    const KEY: u32 = BTRFS_QGROUP_LIMIT_KEY;

    #[inline]
    unsafe fn from_search_unckeched<T>(bufp: *const T) -> Self
    {
        Self(bufp.cast(), PhantomData)
    }
}

impl QgroupLimit<'_>
{
    pub const fn flags(&self) -> u64
    {
        tree_item_get!(__le64(self->flags))
    }

    pub const fn max_rfer(&self) -> u64
    {
        tree_item_get!(__le64(self->max_rfer))
    }

    pub const fn max_excl(&self) -> u64
    {
        tree_item_get!(__le64(self->max_excl))
    }

    pub const fn rsv_rfer(&self) -> u64
    {
        tree_item_get!(__le64(self->rsv_rfer))
    }

    pub const fn rsv_excl(&self) -> u64
    {
        tree_item_get!(__le64(self->rsv_excl))
    }
}

// =========================================================================
// QGROUP RELATION item

pub struct QgroupRelation<'a>(PhantomData<&'a c_char>);

impl<'buf> TreeItem<'buf> for QgroupRelation<'buf>
{
    const KEY: u32 = BTRFS_QGROUP_RELATION_KEY;

    #[inline]
    unsafe fn from_search_unckeched<T>(_bufp: *const T) -> Self
    {
        Self(PhantomData)
    }
}

// =========================================================================
// ORPHAN item

pub struct OrphanItem<'a>(PhantomData<&'a c_char>);

impl<'buf> TreeItem<'buf> for OrphanItem<'buf>
{
    const KEY: u32 = BTRFS_ORPHAN_ITEM_KEY;

    #[inline]
    unsafe fn from_search_unckeched<T>(_bufp: *const T) -> Self
    {
        Self(PhantomData)
    }
}

// =========================================================================
// DIR LOG item

pub struct DirLogItem<'a>(*const btrfs_dir_log_item, PhantomData<&'a c_char>);
pub struct DirLogIndex<'a>(*const btrfs_dir_log_item, PhantomData<&'a c_char>);

macro_rules! impl_for_btrfs_dir_log_item {
    (<$buf:lifetime> $btrfs_dir_log_item:ty => $KEY:expr) => {
        impl<$buf> TreeItem<$buf> for $btrfs_dir_log_item
        {
            const KEY: u32 = $KEY;

            #[inline]
            unsafe fn from_search_unckeched<T>(bufp: *const T) -> Self
            {
                Self(bufp.cast(), PhantomData)
            }
        }

        impl<$buf> $btrfs_dir_log_item
        {
            #[inline(always)]
            pub const fn end(&self) -> u64
            {
                tree_item_get!(__le64(self->end))
            }
        }
    }
}
impl_for_btrfs_dir_log_item!(<'buf> DirLogItem<'buf> => BTRFS_DIR_LOG_ITEM_KEY);
impl_for_btrfs_dir_log_item!(<'buf> DirLogIndex<'buf> => BTRFS_DIR_LOG_INDEX_KEY);

// =========================================================================
// BALANCE item

pub struct TemporaryItem<'a>(*const btrfs_balance_item, PhantomData<&'a c_char>);

impl<'buf> TreeItem<'buf> for TemporaryItem<'buf>
{
    const KEY: u32 = BTRFS_TEMPORARY_ITEM_KEY;

    #[inline]
    unsafe fn from_search_unckeched<T>(bufp: *const T) -> Self
    {
        Self(bufp.cast(), PhantomData)
    }
}

impl TemporaryItem<'_>
{
    pub const fn flags(&self) -> u64
    {
        tree_item_get!(__le64(self->flags))
    }

    pub const fn data(&self) -> DiskBalanceArgs<'_>
    {
        unsafe { DiskBalanceArgs((&raw const (*self.0).data), PhantomData) }
    }

    pub const fn meta(&self) -> DiskBalanceArgs<'_>
    {
        unsafe { DiskBalanceArgs((&raw const (*self.0).meta), PhantomData) }
    }

    pub const fn sys(&self) -> DiskBalanceArgs<'_>
    {
        unsafe { DiskBalanceArgs((&raw const (*self.0).sys), PhantomData) }
    }
}

pub struct DiskBalanceArgs<'a>(*const btrfs_disk_balance_args, PhantomData<&'a c_char>);

impl DiskBalanceArgs<'_>
{
    pub const fn limit(&self) -> u64
    {
        debug_assert!(self.flags() & BTRFS_BALANCE_ARGS_LIMIT != 0);

        unsafe {
            let u = (&raw const (*self.0).inner2).read_unaligned();

            u64::from_le(u.limit)
        }
    }

    pub const fn limit_range(&self) -> (u32, u32)
    {
        debug_assert!(self.flags() & BTRFS_BALANCE_ARGS_LIMIT_RANGE != 0);

        let s = unsafe {
            let p = &raw const (*self.0).inner2.inner1;

            std::ptr::read_unaligned(p)
        };
        (u32::from_le(s.limit_min), u32::from_le(s.limit_max))
    }

    pub const fn usage(&self) -> u64
    {
        debug_assert!(self.flags() & BTRFS_BALANCE_ARGS_USAGE != 0);

        unsafe {
            let u = (&raw const (*self.0).inner1).read_unaligned();

            u64::from_le(u.usage)
        }
    }

    pub const fn usage_range(&self) -> (u32, u32)
    {
        debug_assert!(self.flags() & BTRFS_BALANCE_ARGS_USAGE_RANGE != 0);

        let s = unsafe {
            let p = &raw const (*self.0).inner1.inner1;

            std::ptr::read_unaligned(p)
        };
        (u32::from_le(s.usage_min), u32::from_le(s.usage_max))
    }

    pub const fn profiles(&self) -> u64
    {
        tree_item_get!(__le64(self->profiles))
    }

    pub const fn devid(&self) -> u64
    {
        tree_item_get!(__le64(self->devid))
    }

    pub const fn pstart(&self) -> u64
    {
        tree_item_get!(__le64(self->pstart))
    }

    pub const fn vstart(&self) -> u64
    {
        tree_item_get!(__le64(self->vstart))
    }

    pub const fn vend(&self) -> u64
    {
        tree_item_get!(__le64(self->vend))
    }

    pub const fn target(&self) -> u64
    {
        tree_item_get!(__le64(self->target))
    }

    pub const fn flags(&self) -> u64
    {
        tree_item_get!(__le64(self->flags))
    }

    pub const fn stripes_min(&self) -> u32
    {
        tree_item_get!(__le32(self->stripes_min))
    }

    pub const fn stripes_max(&self) -> u32
    {
        tree_item_get!(__le32(self->stripes_max))
    }
}

// =========================================================================
// UUID item

pub struct UuidSubvol<'a>(PhantomData<&'a c_char>);

impl<'buf> TreeItem<'buf> for UuidSubvol<'buf>
{
    const KEY: u32 = BTRFS_UUID_KEY_SUBVOL as u32;

    unsafe fn from_search_unckeched<T>(_bufp: *const T) -> Self
    {
        Self(PhantomData)
    }
}

pub struct UuidReceivedSubvol<'a>(PhantomData<&'a c_char>);

impl<'buf> TreeItem<'buf> for UuidReceivedSubvol<'buf>
{
    const KEY: u32 = BTRFS_UUID_KEY_RECEIVED_SUBVOL as u32;

    unsafe fn from_search_unckeched<T>(_bufp: *const T) -> Self
    {
        Self(PhantomData)
    }
}
