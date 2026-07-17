#![allow(unused)]
use std::marker::PhantomData;

pub(super) const BTRFS_SEND_STREAM_MAGIC: &[u8; 13] = b"btrfs-stream\0";
pub(super) const BTRFS_SEND_STREAM_VERSION: u32 = 2;

// In send stream v1, no command is larger than 64KiB. In send stream v2, no limit should be assumed.
pub(super) const BTRFS_SEND_BUF_SIZE_V1: usize = 64 * 1024;

#[derive(Debug)]
#[repr(C, packed)]
pub(super) struct StreamHeader
{
    pub magic: [u8; BTRFS_SEND_STREAM_MAGIC.len()],
    pub version: u32,
}

impl From<&[u8]> for StreamHeader
{
    fn from(b: &[u8]) -> Self
    {
        Self {
            magic: [
                b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7], b[8], b[9], b[10], b[11], b[12],
            ],
            version: u32::from_le_bytes([b[13], b[14], b[15], b[16]]),
        }
    }
}

#[derive(Debug)]
#[repr(C, packed)]
pub(crate) struct CmdHeader
{
    pub len: u32,
    pub cmd: u16,
    pub crc: u32,
}

impl From<&[u8]> for CmdHeader
{
    fn from(b: &[u8]) -> Self
    {
        Self {
            len: u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            cmd: u16::from_le_bytes([b[4], b[5]]),
            crc: u32::from_le_bytes([b[6], b[7], b[8], b[9]]),
        }
    }
}

#[derive(Debug)]
pub(super) struct SendAttr<'a>
{
    // In btrfs_tlv_header, this is __le16, but we need 32 bits for attributes with file data as of version 2 of the send stream format.
    pub len: u32,
    pub data: *const u8,
    pub _phantom: PhantomData<&'a u8>,
}

impl SendAttr<'_>
{
    pub fn is_valid(attr: usize) -> bool
    {
        attr != Self::UNSPEC && attr < Self::LEN
    }
    pub const UNSPEC: usize = 0;
    pub const UUID: usize = 1;
    pub const CTRANSID: usize = 2;
    pub const INO: usize = 3;
    pub const SIZE: usize = 4;
    pub const MODE: usize = 5;
    pub const UID: usize = 6;
    pub const GID: usize = 7;
    pub const RDEV: usize = 8;
    pub const CTIME: usize = 9;
    pub const MTIME: usize = 10;
    pub const ATIME: usize = 11;
    pub const OTIME: usize = 12;
    pub const XATTR_NAME: usize = 13;
    pub const XATTR_DATA: usize = 14;
    pub const PATH: usize = 15;
    pub const PATH_TO: usize = 16;
    pub const PATH_LINK: usize = 17;
    pub const FILE_OFFSET: usize = 18;

    // Raw data type is processed in a different way in protocol version 1 and 2. In 1 the maximum
    // size of the TLV is 64KiB as the size is stored in u16. This is not sufficient for encoded write
    // (ENCODED_WRITE) command. In 2 the data length is up to 4GiB (using the type u32) but the TLV
    // must be last and the actual length is calculated as the delta between the whole command and
    // the TLV (i.e. ignoring the TLV header length).
    pub const DATA: usize = 19;
    pub const CLONE_UUID: usize = 20;
    pub const CLONE_CTRANSID: usize = 21;
    pub const CLONE_PATH: usize = 22;
    pub const CLONE_OFFSET: usize = 23;
    pub const CLONE_LEN: usize = 24;

    // Version 2
    pub const FALLOCATE_MODE: usize = 25;

    // File attributes from the FS_*_FL namespace (i_flags, xflags), translated to BTRFS_INODE_* bits
    // (BTRFS_INODE_FLAG_MASK) and stored in btrfs_inode_item::flags (represented by btrfs_inode::flags and btrfs_inode::ro_flags).
    pub const FILEATTR: usize = 26;
    pub const UNENCODED_FILE_LEN: usize = 27;
    pub const UNENCODED_LEN: usize = 28;
    pub const UNENCODED_OFFSET: usize = 29;

    // COMPRESSION and ENCRYPTION default to NONE (0) if omitted from BTRFS_SEND_C_ENCODED_WRITE.
    pub const COMPRESSION: usize = 30;
    pub const ENCRYPTION: usize = 31;

    // Version 3
    pub const VERITY_ALGORITHM: usize = 32;
    pub const VERITY_BLOCK_SIZE: usize = 33;
    pub const VERITY_SALT_DATA: usize = 34;
    pub const VERITY_SIG_DATA: usize = 35;

    pub const LEN: usize = 36;
}

pub(super) mod SendCmd
{
    #![allow(non_snake_case)]
    pub type T = u16;
    pub const UNSPEC: T = 0;
    pub const SUBVOL: T = 1;
    pub const SNAPSHOT: T = 2;
    pub const MKFILE: T = 3;
    pub const MKDIR: T = 4;
    pub const MKNOD: T = 5;
    pub const MKFIFO: T = 6;
    pub const MKSOCK: T = 7;
    pub const SYMLINK: T = 8;
    pub const RENAME: T = 9;
    pub const LINK: T = 10;
    pub const UNLINK: T = 11;
    pub const RMDIR: T = 12;
    pub const SET_XATTR: T = 13;
    pub const REMOVE_XATTR: T = 14;
    pub const WRITE: T = 15;
    pub const CLONE: T = 16;
    pub const TRUNCATE: T = 17;
    pub const CHMOD: T = 18;
    pub const CHOWN: T = 19;
    pub const UTIMES: T = 20;
    pub const END: T = 21;
    pub const UPDATE_EXTENT: T = 22;
    // Version 2
    pub const FALLOCATE: T = 23;
    pub const FILEATTR: T = 24;
    pub const ENCODED_WRITE: T = 25;
}
