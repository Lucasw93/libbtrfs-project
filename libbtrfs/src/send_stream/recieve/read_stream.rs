use super::{receive_stream::ReceiveStream, tlv::*};
use libc::{dev_t, gid_t, mode_t, off_t, uid_t};
use std::{
    io::{self, Read},
    sync::Arc,
    sync::atomic::{AtomicU64, Ordering},
};

pub struct SendStream<'a, R>
{
    pub version: u32,
    pub reader: R,
    pub data_buf: Vec<u8>,
    pub cmd_attrs: [SendAttr<'a>; SendAttr::LEN],

    // end of last successful read, equivalent to start of current malformed part of block
    pub stream_pos: u64,
    pub atomic_pos: Option<Arc<AtomicU64>>,

    #[cfg(feature = "use-crc-fast")]
    pub crc_params: crc_fast::CrcParams,
}

impl<'a, R: Read> SendStream<'a, R>
{
    pub fn read_cmd(&mut self) -> io::Result<SendCmd::T>
    {
        // update the atomic position on a 4M interval
        const BLOCK_MASK: u64 = !0x3F_FFFF;
        const HDR_SZ: usize = size_of::<CmdHeader>();
        unsafe {
            self.cmd_attrs.as_mut_ptr().write_bytes(0, 1);
        }
        self.reader.read_exact(&mut self.data_buf[..HDR_SZ])?;

        let CmdHeader { len, cmd, crc } = CmdHeader::from(&self.data_buf[..]);

        // zero the crc field
        unsafe {
            (*self.data_buf.as_mut_ptr().cast::<CmdHeader>()).crc = 0;
        }
        let data_len = HDR_SZ + len as usize;

        if data_len > self.data_buf.len() {
            self.data_buf.resize_with(data_len, Default::default);
        }
        let data = &mut self.data_buf[..data_len];

        self.reader.read_exact(&mut data[HDR_SZ..])?;

        let checksum = {
            #[cfg(feature = "use-crc-fast")]
            {
                self.crc_params.init = 0;

                crc_fast::checksum_with_params(self.crc_params, data) as u32
            }
            #[cfg(not(feature = "use-crc-fast"))]
            !crc32c::crc32c_append(u32::MAX, data)
        };

        if crc != checksum {
            return receive_error!("crc mismatch in command");
        } else {
            let old_pos = self.stream_pos;
            self.stream_pos += data_len as u64;

            if let Some(ref atomic) = self.atomic_pos {
                if (old_pos & BLOCK_MASK) != (self.stream_pos & BLOCK_MASK) {
                    // update the atomic progress every 4M
                    atomic.store(self.stream_pos, Ordering::Relaxed)
                }
            }
        }

        let mut pos = HDR_SZ;

        while pos < data_len {
            if data_len - pos < size_of::<u16>() {
                return receive_error!("Stream is truncated");
            }
            let tlv_type = u16::from_le_bytes([data[pos], data[pos + 1]]);
            pos += size_of::<u16>();

            if !SendAttr::is_valid(tlv_type as usize) {
                return receive_error!("Invalid tlv in cmd");
            }
            let attr = &mut self.cmd_attrs[tlv_type as usize];

            if self.version >= 2 && tlv_type == SendAttr::DATA as u16 {
                attr.len = (data_len - pos) as u32;
            } else {
                if data_len - pos < size_of::<u16>() {
                    return receive_error!("Stream is truncated (1)");
                }
                attr.len = u16::from_le_bytes([data[pos], data[pos + 1]]) as u32;
                pos += size_of::<u16>();
            }

            if data_len - pos < attr.len as usize {
                return receive_error!("Stream is truncated (2)");
            }
            attr.data = data[pos..].as_ptr();
            pos += attr.len as usize;
        }

        Ok(cmd)
    }
}

#[allow(unused)]
fn process_cmd<R: Read>(
    stream: &mut SendStream<R>,
    receive: &mut ReceiveStream,
) -> io::Result<Option<()>>
{
    match stream.read_cmd()? {
        SendCmd::SUBVOL => {
            let path = stream.tlv_get_path(SendAttr::PATH)?;
            let uuid = stream.tlv_get_uuid(SendAttr::UUID)?;
            let ctransid = stream.tlv_get_u64(SendAttr::CTRANSID)?;

            receive.subvol(path, uuid, ctransid)
        }
        SendCmd::SNAPSHOT => {
            let path = stream.tlv_get_path(SendAttr::PATH)?;
            let uuid = stream.tlv_get_uuid(SendAttr::UUID)?;
            let ctransid = stream.tlv_get_uuid(SendAttr::CTRANSID)?;
            let clone_uuid = stream.tlv_get_uuid(SendAttr::CLONE_UUID)?;
            let clone_ctransid = stream.tlv_get_uuid(SendAttr::CLONE_CTRANSID)?;

            unimplemented!("SNAPSHOT")
        }
        SendCmd::MKFILE => {
            let (path, path_len) = stream.tlv_get_data(SendAttr::PATH)?;
            // ino is not passed to the callbacks in v1
            let ino = stream.tlv_get_u64(SendAttr::INO)?;

            receive.mkfile(path, path_len)
        }
        SendCmd::MKDIR => {
            let (path, path_len) = stream.tlv_get_data(SendAttr::PATH)?;
            // ino is not passed to the callbacks in v1
            let ino = stream.tlv_get_u64(SendAttr::INO)?;

            receive.mkdir(path, path_len)
        }
        SendCmd::MKNOD => {
            let (path, path_len) = stream.tlv_get_data(SendAttr::PATH)?;
            // ino is not passed to the callbacks in v1
            let ino = stream.tlv_get_u64(SendAttr::INO)?;
            let mode = stream.tlv_get_u64(SendAttr::MODE)?;
            let dev = stream.tlv_get_u64(SendAttr::RDEV)?;

            receive.mknod(path, path_len, mode as mode_t, dev as dev_t)
        }
        SendCmd::MKSOCK => {
            let (path, path_len) = stream.tlv_get_data(SendAttr::PATH)?;
            // ino is not passed to the callbacks in v1
            let ino = stream.tlv_get_u64(SendAttr::INO)?;

            receive.mksock(path, path_len)
        }
        SendCmd::SYMLINK => {
            let (target, target_len) = stream.tlv_get_data(SendAttr::PATH)?;
            // ino is not passed to the callbacks in v1
            let ino = stream.tlv_get_u64(SendAttr::INO)?;
            let (lpath, lpath_len) = stream.tlv_get_data(SendAttr::PATH_LINK)?;

            receive.symlink(target, target_len, lpath, lpath_len)
        }
        SendCmd::RENAME => {
            let (from, from_len) = stream.tlv_get_data(SendAttr::PATH)?;
            let (to, to_len) = stream.tlv_get_data(SendAttr::PATH_TO)?;

            receive.rename(from, from_len, to, to_len)
        }
        SendCmd::LINK => {
            let (path, path_len) = stream.tlv_get_data(SendAttr::PATH)?;
            let (link, link_len) = stream.tlv_get_data(SendAttr::PATH_LINK)?;

            receive.link(path, path_len, link, link_len)
        }
        SendCmd::UNLINK => {
            let (path, path_len) = stream.tlv_get_data(SendAttr::PATH)?;

            receive.unlink(path, path_len)
        }
        SendCmd::RMDIR => {
            let (path, path_len) = stream.tlv_get_data(SendAttr::PATH)?;

            receive.rmdir(path, path_len)
        }
        SendCmd::WRITE => {
            let (path, path_len) = stream.tlv_get_data(SendAttr::PATH)?;
            let offset = stream.tlv_get_u64(SendAttr::FILE_OFFSET)?;
            let (data, data_len) = stream.tlv_get_data(SendAttr::DATA)?;

            receive.write(path, path_len, data, data_len, offset as off_t)
        }
        SendCmd::ENCODED_WRITE => {
            let path = stream.tlv_get_path(SendAttr::PATH)?;
            let offset = stream.tlv_get_u64(SendAttr::FILE_OFFSET)?;
            let unencoded_file_len = stream.tlv_get_u64(SendAttr::UNENCODED_FILE_LEN)?;
            let unencoded_len = stream.tlv_get_u64(SendAttr::UNENCODED_LEN)?;
            let unencoded_offset = stream.tlv_get_u64(SendAttr::UNENCODED_OFFSET)?;

            let compression = if stream.cmd_attrs[SendAttr::COMPRESSION].data.is_null() {
                None
            } else {
                Some(stream.tlv_get_u32(SendAttr::COMPRESSION)?)
            };
            let encryption = if stream.cmd_attrs[SendAttr::ENCRYPTION].data.is_null() {
                None
            } else {
                Some(stream.tlv_get_u32(SendAttr::ENCRYPTION)?)
            };
            let (data, len) = stream.tlv_get(SendAttr::DATA)?;

            unimplemented!("ENCODED_WRITE")
        }
        SendCmd::CLONE => {
            unimplemented!("CLONE")
        }
        SendCmd::SET_XATTR => {
            unimplemented!("SET_XATTR")
        }
        SendCmd::REMOVE_XATTR => {
            unimplemented!("REMOVE_XATTR")
        }
        SendCmd::TRUNCATE => {
            let (path, path_len) = stream.tlv_get_data(SendAttr::PATH)?;
            let length = stream.tlv_get_u64(SendAttr::SIZE)?;

            receive.truncate(path, path_len, length as off_t)
        }
        SendCmd::CHMOD => {
            let (path, path_len) = stream.tlv_get_data(SendAttr::PATH)?;
            let mode = stream.tlv_get_u64(SendAttr::MODE)?;

            receive.chmod(path, path_len, mode as mode_t)
        }
        SendCmd::CHOWN => {
            let (path, path_len) = stream.tlv_get_data(SendAttr::PATH)?;
            let uid = stream.tlv_get_u64(SendAttr::UID)?;
            let gid = stream.tlv_get_u64(SendAttr::GID)?;

            receive.chown(path, path_len, uid as uid_t, gid as gid_t)
        }
        SendCmd::UTIMES => {
            let (path, path_len) = stream.tlv_get_data(SendAttr::PATH)?;
            let atime = stream.tlv_get_timespec(SendAttr::ATIME)?;
            let mtime = stream.tlv_get_timespec(SendAttr::MTIME)?;
            let ctime = stream.tlv_get_timespec(SendAttr::CTIME)?;

            receive.utimes(path, path_len, atime, mtime)
        }
        SendCmd::UPDATE_EXTENT => {
            let path = stream.tlv_get_path(SendAttr::PATH)?;
            let offset = stream.tlv_get_u64(SendAttr::FILE_OFFSET)?;
            let tmp = stream.tlv_get_u64(SendAttr::SIZE)?;

            receive.update_extent(path, offset, tmp)
        }
        SendCmd::END => {
            /* end of send stream */

            receive.finish_send()
        }
        SendCmd::FALLOCATE => {
            unimplemented!("FALLOCATE")
        }
        SendCmd::FILEATTR => {
            unimplemented!("FILEATTR")
        }
        _ => unimplemented!(),
    }
}

pub fn process_send_stream<R: Read>(
    receive: &mut ReceiveStream,
    stream: &mut SendStream<R>,
) -> io::Result<()>
{
    stream
        .reader
        .read_exact(&mut stream.data_buf[..size_of::<StreamHeader>()])?;

    let StreamHeader { magic, version } = StreamHeader::from(&stream.data_buf[..]);

    if &magic != BTRFS_SEND_STREAM_MAGIC {
        return receive_error!("Unexpected header");
    } else if version > BTRFS_SEND_STREAM_VERSION {
        return receive_error!("Version not supported");
    }
    stream.version = version;

    // DEBUG
    //eprintln!(
    //    "{}\n{}",
    //    unsafe { std::str::from_utf8_unchecked(&magic) },
    //    stream.version
    //);

    while process_cmd(stream, receive)?.is_some() {
        continue;
    }

    if let Some(ref atomic) = stream.atomic_pos {
        atomic.store(stream.stream_pos, Ordering::Relaxed);
    }

    Ok(())
}
