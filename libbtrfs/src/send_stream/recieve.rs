use super::{handler::StreamHandler, read_stream::SendStream};
use crate::util::IoResult;
use std::{
    io::Read,
    sync::{Arc, atomic::AtomicU64},
};

/// Receives a BTRFS stream.
///
/// This function receives a BTRFS stream using the provided `handler`. The provided stream source
/// must implement [`Read`].
///
/// If the `position` argument is `Some`, the position (bytes read), of the stream will be updated
/// on 4MiB intervals, via the [`AtomicU64`].
///
/// The stream can be read using a additional buffers. For a non-buffered receive, the stream is
/// read into a single buffer, which is then passed to the handler. This means that subsequent reads
/// from the stream must wait for the handler to finish before reading from the stream. If
/// `buffered` is [`true`], an additional buffer will be used so that as soon as the the first
/// buffer is passed to the handler, the main thread does not need to wait for the handler thread to
/// continue reading from the stream.
///
/// A buffered receive will mainly be useful when reads from the stream are are going to be very
/// fast, ie, when the stream is saved on disk. When the stream source is BTRFS_SEND_IOC, reads
/// will end up waiting for the ioctl, more that the handler.
pub fn receive_stream<H: StreamHandler, S: Read>(
    handler: H,
    src: S,
    position: Option<Arc<AtomicU64>>,
    buffered: bool,
) -> IoResult<()>
{
    if buffered {
        SendStream::new(src, position).read_and_handle_buffered(handler)
    } else {
        SendStream::new(src, position).read_and_handle(handler)
    }
}
