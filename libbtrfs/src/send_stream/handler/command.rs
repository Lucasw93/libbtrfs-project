use crate::util::IoResult;
use std::range::Range;

/// Implemented by all send commands
pub trait SendCmd<'a>: Sized
{
    /// The unique command ID, for each send command.
    const KEY: u16;

    /// Parses the send stream and constructs the send command.
    fn parse_tlv(stream: &'a [u8]) -> IoResult<Self>;
}

/// Implemented by commands that have the `data` attribute.
///
/// Send commands that have a `data` attribute implement this trait because the `data` attribute is
/// handled differently in a version 2 send stream.
///
/// For the exact differences please see:
/// **[btrfs.readthedocs.io - dev-send-stream#special-cases](https://btrfs.readthedocs.io/en/latest/dev/dev-send-stream.html#special-cases)**
pub trait SendDataCmd<'a>: SendCmd<'a>
{
    /// Parses the send stream and constructs the send command.
    fn parse_tlv_v2(stream: &'a [u8]) -> IoResult<Self>;
}

macro_rules! SEND_CMD {
    (
        $(#[doc = $doc:literal])+
        struct $name:ident
        {
            const KEY = $key:literal;

            $( $attr_name:ident : $attr_type:ident ; )+
        }
    ) => {

        $(#[doc = $doc])+
        pub struct $name<'a>
        {
            $(#[allow(missing_docs)] pub $attr_name : SEND_CMD!(@type $attr_type)),+
        }

        impl<'a> SendCmd<'a> for $name<'a>
        {
            const KEY: u16 = $key;

            fn parse_tlv(payload: &'a [u8]) -> ::std::io::Result<Self> {
                $(
                    let mut $attr_name: Option<SEND_CMD!(@type $attr_type)> = None;
                )+
                let Range { mut start, mut end } = Range { start: 0, end: size_of::<u32>() };

                while let Some(&[tlo, thi, llo, lhi]) = payload.get(start..end) {
                    let tlv_type = u16::from_le_bytes([tlo, thi]);
                    let tlv_len = u16::from_le_bytes([llo, lhi]) as usize;

                    if let Some(val) = payload.get(end..end + tlv_len) {
                        match tlv_type {
                            $(
                                SEND_CMD!(@attr $attr_name) => {
                                    $attr_name = Some(SEND_CMD!(@parse $attr_type, val))
                                }
                            )+
                            _ => {}
                        }
                    } else {
                        return receive_error!("send stream is trucated")
                    }
                    start = end + tlv_len;
                    end = start + size_of::<u32>();
                }

                #[allow(unused_parens)] // extra parens for structs with only a single field
                if let ( $(Some( $attr_name )),+ ) = ( $( $attr_name ),+ ) {
                    Ok(Self { $( $attr_name ),+ })
                } else {
                    receive_error!("tlv missing attributes")
                }
            }
        }
    };

    ($(#[doc = $doc:literal])+ struct $name:ident { const KEY = $key:literal; }) => {
        $(#[doc = $doc])+
        pub struct $name;

        impl<'a> SendCmd<'a> for $name
        {
            const KEY: u16 = $key;

            fn parse_tlv(_: &'a [u8]) -> ::std::io::Result<Self> {
                Ok(Self)
            }
        }
    };

    // ====================================================================================
    // Data Types

    (@type U8) => { u8 };
    (@type U16) => { u16 };
    (@type U32) => { u32 };
    (@type U64) => { u64 };
    (@type Data) => { &'a [u8] };
    (@type String) => { &'a [u8] };
    (@type Uuid) => { ::uuid::Uuid };
    (@type Timespec) => { ::libc::timespec };

    // ====================================================================================
    // Parse Data Types

    (@parse Int, $t:ty, $value:ident) => {
        if let Ok(bytes) = $value.try_into() {
            <$t>::from_le_bytes(bytes)
        } else {
            return receive_error!("Invalid length for tlv(Int)")
        }
    };
    (@parse U8, $value:ident) => { SEND_CMD!(@parse Int, u8, $value) };
    (@parse U16, $value:ident) => { SEND_CMD!(@parse Int, u16, $value) };
    (@parse U32, $value:ident) => { SEND_CMD!(@parse Int, u32, $value) };
    (@parse U64, $value:ident) => { SEND_CMD!(@parse Int, u64, $value) };
    (@parse Data, $value:ident) => { $value };
    (@parse String, $value:ident) => { $value };
    (@parse Uuid, $value:ident) => {
        if let Ok(uuid) = ::uuid::Uuid::from_slice($value) {
            uuid
        } else {
            return receive_error!("Invalid length for tlv(uuid)")
        }
    };
    (@parse Timespec, $value:ident) => {
        if $value.len() != size_of::<$crate::bindings::btrfs_timespec>() {
            return receive_error!("Invalid length for tlv(timespec)")
        } else {
            let (sec, nsec) = $value.split_at(size_of::<u64>());

            libc::timespec {
                tv_sec: u64::from_le_bytes(sec.try_into().unwrap()) as libc::time_t,
                tv_nsec: u32::from_le_bytes(nsec.try_into().unwrap()) as _,
            }
        }
    };

    // ====================================================================================
    // Attributes

    (@attr uuid) => { 1 };
    (@attr ctransid) => { 2 };
    (@attr ino) => { 3 };
    (@attr size) => { 4 };
    (@attr mode) => { 5 };
    (@attr uid) => { 6 };
    (@attr gid) => { 7 };
    (@attr rdev) => { 8 };
    (@attr ctime) => { 9 };
    (@attr mtime) => { 10 };
    (@attr atime) => { 11 };
    (@attr otime) => { 12 };
    (@attr xattr_name) => { 13 };
    (@attr xattr_data) => { 14 };
    (@attr path) => { 15 };
    (@attr path_to) => { 16 };
    (@attr path_link) => { 17 };
    (@attr file_offset) => { 18 };
    (@attr data) => { 19 };
    (@attr clone_uuid) => { 20 };
    (@attr clone_ctransid) => { 21 };
    (@attr clone_path) => { 22 };
    (@attr clone_offset) => { 23 };
    (@attr clone_len) => { 24 };
    // Version 2
    (@attr fallocate_mode) => { 25 };
    (@attr fileattr) => { 26 };
    (@attr unencoded_file_len) => { 27 };
    (@attr unencoded_len) => { 28 };
    (@attr unencoded_offset) => { 29 };
    (@attr compression) => { 30 };
    (@attr encryption) => { 31 };
}

SEND_CMD! {
    /// Handled by, [super::StreamHandler::subvol()]
    struct SubvolCmd {
        const KEY = 1;

        path: String;
        ctransid: U64;
        uuid: Uuid;
    }
}

SEND_CMD! {
    /// Handled by, [super::StreamHandler::snapshot()]
    struct SnapshotCmd
    {
        const KEY = 2;

        path: String;
        uuid: Uuid;
        ctransid: U64;
        clone_uuid: Uuid;
        clone_ctransid: U64;
    }
}

SEND_CMD! {
    /// Handled by, [super::StreamHandler::mkfile()]
    struct MkfileCmd
    {
        const KEY = 3;

        path: String;
        ino: U64;
    }
}

SEND_CMD! {
    /// Handled by, [super::StreamHandler::mkdir()]
    struct MkdirCmd
    {
        const KEY = 4;

        path: String;
        ino: U64;
    }
}

SEND_CMD! {
    /// Handled by, [super::StreamHandler::mknod()]
    struct MknodCmd
    {
        const KEY = 5;

        path: String;
        mode: U64;
        rdev: U64;
    }
}

SEND_CMD! {
    /// Handled by, [super::StreamHandler::mkfifo()]
    struct MkfifoCmd
    {
        const KEY = 6;

        path: String;
        ino: U64;
    }
}

SEND_CMD! {
    /// Handled by, [super::StreamHandler::mksock()]
    struct MksockCmd
    {
        const KEY = 7;

        path: String;
        ino: U64;
    }
}

SEND_CMD! {
    /// Handled by, [super::StreamHandler::symlink()]
    struct SymlinkCmd
    {
        const KEY = 8;

        path: String;
        ino: U64;
        path_link: String;
    }
}

SEND_CMD! {
    /// Handled by, [super::StreamHandler::rename()]
    struct RenameCmd
    {
        const KEY = 9;

        path: String;
        path_to: String;
    }
}

SEND_CMD! {
    /// Handled by, [super::StreamHandler::link()]
    struct LinkCmd
    {
        const KEY = 10;

        path: String;
        path_link: String;
    }
}

SEND_CMD! {
    /// Handled by, [super::StreamHandler::unlink()]
    struct UnlinkCmd
    {
        const KEY = 11;

        path: String;
    }
}

SEND_CMD! {
    /// Handled by, [super::StreamHandler::rmdir()]
    struct RmdirCmd
    {
        const KEY = 12;

        path: String;
    }
}

SEND_CMD! {
    /// Handled by, [super::StreamHandler::set_xattr()]
    struct SetXattrCmd
    {
        const KEY = 13;

        path: String;
        xattr_name: String;
        xattr_data: Data;
    }
}

SEND_CMD! {
    /// Handled by, [super::StreamHandler::remove_xattr()]
    struct RemoveXattrCmd
    {
        const KEY = 14;

        path: String;
        xattr_name: String;
    }
}

SEND_CMD! {
    /// Handled by, [super::StreamHandler::write()]
    struct WriteCmd
    {
        const KEY = 15;

        path: String;
        file_offset: U64;
        data: Data;
    }
}

impl<'a> SendDataCmd<'a> for WriteCmd<'a>
{
    fn parse_tlv_v2(payload: &'a [u8]) -> std::io::Result<Self>
    {
        let mut path: Option<&[u8]> = None;
        let mut file_offset: Option<u64> = None;
        let mut data: Option<&[u8]> = None;

        let Range { mut start, mut end } = Range { start: 0, end: size_of::<u16>() };

        while let Some(&[lo, hi]) = payload.get(start..end) {
            let ty = u16::from_le_bytes([lo, hi]);

            if ty == SEND_CMD!(@attr data) {
                data = payload.get(end..);

                break;
            } else {
                start += size_of::<u16>();
                end += size_of::<u16>();

                let len = match payload.get(start..end) {
                    Some(&[lo, hi]) => u16::from_le_bytes([lo, hi]) as usize,
                    _ => panic!("parsing write tlv"),
                };

                if let Some(val) = payload.get(end..end + len) {
                    match ty {
                        SEND_CMD!(@attr path) => path = Some(val),
                        SEND_CMD!(@attr file_offset) => {
                            file_offset = Some(SEND_CMD!(@parse U64, val))
                        }
                        _ => {}
                    }
                }
                start = end + len;
                end = start + size_of::<u16>();
            }
        }

        if let (Some(path), Some(file_offset), Some(data)) = (path, file_offset, data) {
            Ok(Self { path, file_offset, data })
        } else {
            receive_error!("tlv missing attributes")
        }
    }
}

SEND_CMD! {
    /// Handled by, [super::StreamHandler::clone()]
    struct CloneCmd
    {
        const KEY = 16;

        path: String;
        file_offset: U64;
        clone_len: U64;
        clone_uuid: Uuid;
        clone_ctransid: U64;
        clone_path: String;
        clone_offset: U64;
    }
}

SEND_CMD! {
    /// Handled by, [super::StreamHandler::truncate()]
    struct TruncateCmd
    {
        const KEY = 17;

        path: String;
        size: U64;
    }
}

SEND_CMD! {
    /// Handled by, [super::StreamHandler::chmod()]
    struct ChmodCmd
    {
        const KEY = 18;

        path: String;
        mode: U64;
    }
}

SEND_CMD! {
    /// Handled by, [super::StreamHandler::chown()]
    struct ChownCmd
    {
        const KEY = 19;

        path: String;
        uid: U64;
        gid: U64;
    }

}

SEND_CMD! {
    /// Handled by, [super::StreamHandler::utimes()]
    struct UtimesCmd
    {
        const KEY = 20;

        path: String;
        atime: Timespec;
        mtime: Timespec;
        ctime: Timespec;

        // NOTE: otime not sent in version 1
        //otime: Timespec;
    }
}

SEND_CMD! {
    /// Handled by, [super::StreamHandler::end()]
    struct EndCmd {
        const KEY = 21;
    }
}

SEND_CMD! {
    /// Handled by, [super::StreamHandler::update_extent()]
    struct UpdateExtentCmd
    {
        const KEY = 22;

        path: String;
        file_offset: U64;
        size: U64;
    }
}

// =======================================================================================
// VERSION 2
// =======================================================================================

SEND_CMD! {
    /// Handled by, [super::StreamHandler::fallocate()]
    struct FallocateCmd
    {
        const KEY = 23;

        path: String;
        fallocate_mode: U32;
        file_offset: U64;
        size: U64;
    }
}

SEND_CMD! {
    /// Handled by, [super::StreamHandler::fileattr()]
    struct FileattrCmd {
        const KEY = 24;

        path: String;
        fileattr: U64;
    }
}

// ==========================================================
// SEND_CMD Encoded Write

/// Handled by, [super::StreamHandler::encoded_write()]
#[allow(missing_docs)]
pub struct EncodedWriteCmd<'a>
{
    pub path: &'a [u8],
    pub file_offset: u64,
    pub unencoded_file_len: u64,
    pub unencoded_len: u64,
    pub unencoded_offset: u64,
    pub compression: Option<u32>,
    pub encryption: Option<u32>,
    pub data: &'a [u8],
}

impl<'a> SendCmd<'a> for EncodedWriteCmd<'a>
{
    const KEY: u16 = 25;

    fn parse_tlv(_: &'a [u8]) -> IoResult<Self>
    {
        unreachable!("version 2 only data command")
    }
}

impl<'a> SendDataCmd<'a> for EncodedWriteCmd<'a>
{
    fn parse_tlv_v2(payload: &'a [u8]) -> std::io::Result<Self>
    {
        let mut path: Option<&[u8]> = None;
        let mut file_offset: Option<u64> = None;
        let mut unencoded_file_len: Option<u64> = None;
        let mut unencoded_len: Option<u64> = None;
        let mut unencoded_offset: Option<u64> = None;
        let mut compression: Option<u32> = None;
        let mut encryption: Option<u32> = None;
        let mut data: Option<&[u8]> = None;

        let Range { mut start, mut end } = Range { start: 0, end: size_of::<u16>() };

        while let Some(&[lo, hi]) = payload.get(start..end) {
            let ty = u16::from_le_bytes([lo, hi]);

            if ty == SEND_CMD!(@attr data) {
                data = payload.get(end..);

                break;
            } else {
                start += size_of::<u16>();
                end += size_of::<u16>();

                let len = match payload.get(start..end) {
                    Some(&[lo, hi]) => u16::from_le_bytes([lo, hi]) as usize,
                    _ => panic!("parsing write tlv"),
                };

                if let Some(val) = payload.get(end..end + len) {
                    match ty {
                        SEND_CMD!(@attr path) => path = Some(val),
                        SEND_CMD!(@attr file_offset) => {
                            file_offset = Some(SEND_CMD!(@parse U64, val))
                        }
                        SEND_CMD!(@attr unencoded_file_len) => {
                            unencoded_file_len = Some(SEND_CMD!(@parse U64, val))
                        }
                        SEND_CMD!(@attr unencoded_len) => {
                            unencoded_len = Some(SEND_CMD!(@parse U64, val))
                        }
                        SEND_CMD!(@attr unencoded_offset) => {
                            unencoded_offset = Some(SEND_CMD!(@parse U64, val))
                        }

                        SEND_CMD!(@attr compression) => {
                            compression = Some(SEND_CMD!(@parse U32, val))
                        }
                        SEND_CMD!(@attr encryption) => {
                            encryption = Some(SEND_CMD!(@parse U32, val))
                        }
                        _ => {}
                    }
                }
                start = end + len;
                end = start + size_of::<u16>();
            }
        }

        if let (
            Some(path),
            Some(file_offset),
            Some(unencoded_file_len),
            Some(unencoded_len),
            Some(unencoded_offset),
            Some(data),
        ) = (
            path,
            file_offset,
            unencoded_file_len,
            unencoded_len,
            unencoded_offset,
            data,
        ) {
            Ok(Self {
                path,
                file_offset,
                unencoded_file_len,
                unencoded_len,
                unencoded_offset,
                compression,
                encryption,
                data,
            })
        } else {
            receive_error!("tlv missing attributes")
        }
    }
}
