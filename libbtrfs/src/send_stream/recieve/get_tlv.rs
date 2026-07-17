use super::read_stream::SendStream;
use crate::bindings::btrfs_timespec;
use libc::{time_t, timespec};
use std::{
    io::{self, Read},
    mem::size_of,
    slice,
};
use uuid::Uuid;

macro_rules! tlv_check_len {
    ($expected:expr, $got:expr) => {
        if $expected != $got {
            return Err(::std::io::Error::new(
                ::std::io::ErrorKind::Other,
                format!(
                    "[RECEIVE-ERROR]: invalid size for attribue, expected: {}, got: {}",
                    $expected, $got
                ),
            ));
        }
    };
}

macro_rules! tlv_get_int {
    ($stream:expr, $attr:expr, $ty:ty) => {{
        let (data, len) = $stream.tlv_get($attr)?;

        tlv_check_len!(size_of::<$ty>(), len);

        Ok(<$ty>::from_le_bytes(data.try_into().unwrap()))
    }};
}

#[allow(unused)]
impl<R: Read> SendStream<'_, R>
{
    pub(super) fn tlv_get_u8(&self, attr: usize) -> io::Result<u8>
    {
        tlv_get_int!(self, attr, u8)
    }

    pub(super) fn tlv_get_u16(&self, attr: usize) -> io::Result<u16>
    {
        tlv_get_int!(self, attr, u16)
    }

    pub(super) fn tlv_get_u32(&self, attr: usize) -> io::Result<u32>
    {
        tlv_get_int!(self, attr, u32)
    }

    pub(super) fn tlv_get_u64(&self, attr: usize) -> io::Result<u64>
    {
        tlv_get_int!(self, attr, u64)
    }

    pub(super) fn tlv_get_path(&self, attr: usize) -> io::Result<&[u8]>
    {
        self.tlv_get(attr).map(|(data, _)| data)
    }

    pub(super) fn tlv_get_uuid(&self, attr: usize) -> io::Result<Uuid>
    {
        let (data, len) = self.tlv_get(attr)?;

        match Uuid::from_slice(data) {
            Ok(uuid) => Ok(uuid),
            Err(_) => receive_error!("Invalid size for UUID attribue"),
        }
    }

    pub(super) fn tlv_get_timespec(&self, attr: usize) -> io::Result<timespec>
    {
        let (data, len) = self.tlv_get(attr)?;

        tlv_check_len!(size_of::<btrfs_timespec>(), len);

        let (sec, nsec) = data.split_at(size_of::<u64>());

        Ok(timespec {
            tv_sec: u64::from_le_bytes(sec.try_into().unwrap()) as time_t,
            tv_nsec: u32::from_le_bytes(nsec.try_into().unwrap()).into(),
        })
    }

    pub(super) fn tlv_get(&self, attr: usize) -> io::Result<(&[u8], usize)>
    {
        let send_attr = &self.cmd_attrs[attr];

        if send_attr.data.is_null() {
            return receive_error!("Requested attribute not present");
        }

        let data_len = send_attr.len as usize;

        let data = unsafe { slice::from_raw_parts(send_attr.data, data_len) };

        Ok((data, data_len))
    }

    pub(super) fn tlv_get_data(&self, attr: usize) -> io::Result<(*const u8, usize)>
    {
        let send_attr = &self.cmd_attrs[attr];

        if send_attr.data.is_null() {
            return receive_error!("Requested attribute not present");
        }

        Ok((send_attr.data, send_attr.len as usize))
    }
}
