#![allow(missing_docs)]
use crate::util::{IoError, IoResult};
use bitflags::bitflags;
use std::io::ErrorKind;

bitflags! {
    /// Flags
    ///
    /// This struct contains flags that are used by all funtions in the library.
    #[derive(Debug)]
    pub struct Flags: u64 {
        const NONE = 0x0;

        // ==========================================================
        // Subvolume Iterator Flags

        const ASCENDING = 0x0;
        const DESCENDING = 0x1;
        const GET_INFO = 0x2;
        const GET_PATH = 0x4;

        // ==========================================================
        // Fs Info Flags

        const CSUM_INFO = 0x8;
        const GENERATION = 0x10;
        const METADATA_UUID = 0x20;

        // ==========================================================
        // Send Flags

        const NO_FILE_DATA = 0x40;
        const OMIT_STREAM_HEADER = 0x80;
        const OMIT_END_CMD = 0x100;
        const VERSION = 0x200;
        const COMPRESSED = 0x400;
    }
}

impl Default for Flags
{
    fn default() -> Self
    {
        Flags::NONE
    }
}
macro_rules! build_raw_flag {
    ($__self:expr, $( $flag:ident => $raw:ident ),+) => {{
        let mut bits = 0;
        $(if $__self.contains(Self::$flag) {
            bits |= $crate::bindings::$raw;
        })+
        bits
    }}
}

impl Flags
{
    fn check_invalid(&self, mask: u64) -> IoResult<()>
    {
        let invalid_bits = self.bits() & !mask;

        if invalid_bits != 0 {
            let flags = Flags::from_bits_truncate(invalid_bits);
            let msg = format!("Invalid flags: {flags:?}");

            return Err(IoError::new(ErrorKind::InvalidInput, msg));
        }

        Ok(())
    }

    const FS_INFO_MASK: u64 =
        Flags::CSUM_INFO.bits() | Flags::GENERATION.bits() | Flags::METADATA_UUID.bits();

    pub(crate) fn to_raw_fs_info_flags(self) -> IoResult<u64>
    {
        self.check_invalid(Self::FS_INFO_MASK)?;

        let flags = build_raw_flag![
            self,
            CSUM_INFO => BTRFS_FS_INFO_FLAG_CSUM_INFO,
            GENERATION => BTRFS_FS_INFO_FLAG_GENERATION,
            METADATA_UUID => BTRFS_FS_INFO_FLAG_METADATA_UUID
        ];

        Ok(flags)
    }

    const SEND_MASK: u64 = Flags::NO_FILE_DATA.bits()
        | Flags::OMIT_STREAM_HEADER.bits()
        | Flags::OMIT_END_CMD.bits()
        | Flags::VERSION.bits()
        | Flags::COMPRESSED.bits();

    pub(crate) fn to_raw_send_flags(self) -> IoResult<u64>
    {
        self.check_invalid(Self::SEND_MASK)?;

        let flags = build_raw_flag![
            self,
            NO_FILE_DATA => BTRFS_SEND_FLAG_NO_FILE_DATA,
            OMIT_STREAM_HEADER => BTRFS_SEND_FLAG_OMIT_STREAM_HEADER,
            OMIT_END_CMD => BTRFS_SEND_FLAG_OMIT_END_CMD,
            VERSION => BTRFS_SEND_FLAG_VERSION,
            COMPRESSED => BTRFS_SEND_FLAG_COMPRESSED
        ];

        Ok(flags)
    }
}
