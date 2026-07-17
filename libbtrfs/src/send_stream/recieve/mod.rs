macro_rules! receive_error {
    ( $msg:literal ) => {
        Err(::std::io::Error::new(
            ::std::io::ErrorKind::Other,
            format!("[RECEIVE-ERROR]: {}", $msg),
        ))
    };
}

mod get_tlv;
mod read_stream;
mod receive_stream;
mod tlv;

use read_stream::{SendStream, process_send_stream};
use receive_stream::ReceiveStream;
use tlv::BTRFS_SEND_BUF_SIZE_V1;

use crate::util::IoResult;

use std::{
    io::Read,
    mem::MaybeUninit,
    path::Path,
    sync::{Arc, atomic::AtomicU64},
};

pub fn receive_stream<P: AsRef<Path>, S: Read>(
    dst: P,
    src: S,
    progress: Option<Arc<AtomicU64>>,
) -> IoResult<()>
{
    if !dst.as_ref().exists() {
        return receive_error!("Invalid destination");
    }
    let mut rctx = ReceiveStream {
        destination: dst.as_ref().to_path_buf(),
        ..Default::default()
    };

    let mut sctx = SendStream {
        reader: src,
        data_buf: vec![0; BTRFS_SEND_BUF_SIZE_V1],
        cmd_attrs: unsafe { MaybeUninit::zeroed().assume_init() },
        version: 0,
        stream_pos: 0,
        atomic_pos: progress,

        #[cfg(feature = "use-crc-fast")]
        crc_params: btrfs_send_crc32_parms(),
    };

    process_send_stream(&mut rctx, &mut sctx)
}

#[cfg(feature = "use-crc-fast")]
#[inline(always)]
pub fn btrfs_send_crc32_parms() -> crc_fast::CrcParams
{
    crc_fast::CrcParams::new(
        "BTRFS-CRC",
        32,
        0x1EDC6F41,
        0x00000000,
        true,
        0x00000000,
        0x58E3FA20,
    )
}

#[cfg(feature = "use-crc-fast")]
#[test]
pub fn check_btrfs_send_crc32_parms()
{
    let parms = btrfs_send_crc32_parms();
    let checksum = crc_fast::checksum_with_params(parms, b"123456789");

    assert_eq!(checksum, parms.check);
}
