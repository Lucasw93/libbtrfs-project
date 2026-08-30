use super::handler::StreamHandler;
use crate::{
    bindings::{BTRFS_IOC_SEND, BTRFS_SEND_FLAG_VERSION, btrfs_ioctl_send_args},
    fs::fd::is_btrfs,
    util::btrfs_ioctl,
    util::send::supported_send_version,
};
use libc::F_SETPIPE_SZ;
use std::{
    fs::File,
    io::{self, ErrorKind, PipeReader, PipeWriter, pipe, stdout},
    mem::MaybeUninit,
    os::fd::{AsFd, AsRawFd, OwnedFd},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    thread::{self, JoinHandle},
};

/// Joinable handle returned by [`SendBuilder::spawn_send()`].
pub struct SendHandle
{
    position: Arc<AtomicU64>,
    send_handle: JoinHandle<io::Result<()>>,
    recv_handle: Option<JoinHandle<io::Result<()>>>,
}

impl SendHandle
{
    /// Joins both the sending and receiving threads.
    pub fn join(self) -> io::Result<()>
    {
        if let Some(handle) = self.recv_handle {
            handle.join().unwrap()?;
        }
        self.send_handle.join().unwrap()
    }

    /// Check if the both the sending and receiving threads have finishd.
    pub fn is_finished(&self) -> bool
    {
        self.send_is_finished() && self.receive_is_finished()
    }

    /// Check if the receiving thread is finished.
    pub fn receive_is_finished(&self) -> bool
    {
        self.recv_handle
            .as_ref()
            .is_none_or(JoinHandle::is_finished)
    }

    /// Check if the sending thread is finished.
    pub fn send_is_finished(&self) -> bool
    {
        self.send_handle.is_finished()
    }

    /// Load the current position of the send stream.
    pub fn position(&self) -> u64
    {
        self.position.load(Ordering::Relaxed)
    }
}

/// Builds a send stream.
pub struct SendBuilder<R: AsFd, H>
{
    source: R,
    buffered: bool,
    version: u32,
    send_fd: i64,
    flags: u64,
    handler: Option<H>,
}

impl<R: AsFd, H> From<R> for SendBuilder<R, H>
{
    fn from(source: R) -> Self
    {
        Self {
            source,
            buffered: false,
            version: supported_send_version(),
            send_fd: stdout().as_raw_fd() as i64,
            flags: 0,
            handler: None,
        }
    }
}

impl<H> SendBuilder<OwnedFd, H>
{
    /// Construes a new builder from a given path.
    pub fn from_path<P: AsRef<Path>>(path: P) -> io::Result<Self>
    {
        Ok(Self::from(OwnedFd::from(File::open(path)?)))
    }
}

macro_rules! send_flag_to_builder_fn {
    {
        $(#[doc = $cmt:literal])+
        pub fn ($name:ident | $flag:ident) -> Self;
    } => {
        $(#[doc = $cmt])+
        pub fn $name(mut self, $name: bool) -> Self
        {
            const FLAG: u64 = $crate::bindings::$flag;

            if $name {
                self.flags |= FLAG;
            } else {
                self.flags &= !FLAG;
            }
            self
        }
    }
}

impl<R, H> SendBuilder<R, H>
where
    H: StreamHandler + Send + 'static,
    R: AsFd + Send + 'static,
{
    const BUFSZ_V1: i32 = 64_000;
    const BUFSZ_V2: i32 = 128_000;

    /// Construes a new builder.
    pub fn new(source: R) -> Self
    {
        Self::from(source)
    }

    /// Set the send stream version.
    ///
    /// By default the highest supported version if used.
    pub fn version(mut self, version: u32) -> Self
    {
        self.version = version;
        self
    }

    /// Use the given `handler` to receive the stream.
    pub fn handler(mut self, handler: H) -> Self
    {
        self.handler = Some(handler);
        self
    }

    /// Wether or not to use a buffered receive.
    ///
    /// This options has no effect if no handler is provided.
    ///
    /// See, [super::receive_stream()] for more information.
    pub fn buffered(mut self, buffered: bool) -> Self
    {
        self.buffered = buffered;
        self
    }

    send_flag_to_builder_fn! {
        /// File data wont be included in the stream.
        ///
        /// Write commands will be replaced with Update Extent commands.
        pub fn (no_file_data | BTRFS_SEND_FLAG_NO_FILE_DATA) -> Self;
    }

    send_flag_to_builder_fn! {
        /// Omit the command at the end of the stream.
        pub fn (omit_end_cmd | BTRFS_SEND_FLAG_OMIT_END_CMD) -> Self;
    }

    send_flag_to_builder_fn! {
        /// Omit the stream header.
        pub fn (omit_stream_header | BTRFS_SEND_FLAG_OMIT_STREAM_HEADER) -> Self;
    }

    send_flag_to_builder_fn! {
        /// Send compressed data.
        pub fn (compressed | BTRFS_SEND_FLAG_COMPRESSED) -> Self;
    }

    /// Consumes the builder and spawns a non blocking send.
    pub fn spawn_send(mut self) -> io::Result<SendHandle>
    {
        let (reader, writer): (PipeReader, PipeWriter);

        let recv_data = if let Some(handler) = self.handler {
            (reader, writer) = pipe()?;

            self.send_fd = writer.as_raw_fd() as i64;

            let pipe_sz = if self.version > 1 { Self::BUFSZ_V2 } else { Self::BUFSZ_V1 };
            syscall!(unsafe { fcntl(reader.as_raw_fd(), F_SETPIPE_SZ, pipe_sz) })?;

            Some((handler, reader, writer))
        } else {
            None
        };

        if !is_btrfs(self.source.as_fd())? {
            return Err(ErrorKind::Unsupported.into());
        }

        let send_handle = thread::spawn(move || {
            let fd = self.source;
            let mut args: btrfs_ioctl_send_args = unsafe { MaybeUninit::zeroed().assume_init() };

            args.send_fd = self.send_fd;
            args.flags = self.flags;
            args.version = self.version;
            if args.version > 1 {
                args.flags |= BTRFS_SEND_FLAG_VERSION
            }
            btrfs_ioctl(fd.as_fd(), BTRFS_IOC_SEND, &mut args)
        });

        let position = Arc::default();
        let position_clone = Arc::clone(&position);

        let recv_handle = recv_data.map(|(handler, reader, writer)| {
            thread::spawn(move || {
                let _w = writer; // dont close the pipe

                super::receive_stream(handler, reader, Some(position_clone), self.buffered)
            })
        });

        Ok(SendHandle { position, recv_handle, send_handle })
    }

    /// Consumes the builder and performs a blocking send.
    pub fn blocking_send(mut self) -> io::Result<()>
    {
        let (reader, writer): (PipeReader, PipeWriter);

        let recv_handle = if let Some(handler) = self.handler {
            (reader, writer) = pipe()?;

            self.send_fd = writer.as_raw_fd() as i64;

            let pipe_sz = if self.version > 1 { Self::BUFSZ_V2 } else { Self::BUFSZ_V1 };
            syscall!(unsafe { fcntl(reader.as_raw_fd(), F_SETPIPE_SZ, pipe_sz) })?;

            Some(thread::spawn(move || {
                super::receive_stream(handler, reader, None, self.buffered)
            }))
        } else {
            None
        };

        let mut args: btrfs_ioctl_send_args = unsafe { MaybeUninit::zeroed().assume_init() };

        args.send_fd = self.send_fd;
        args.flags = self.flags;
        args.version = self.version;
        if args.version > 1 {
            args.flags |= BTRFS_SEND_FLAG_VERSION
        }
        let send_result = btrfs_ioctl(self.source.as_fd(), BTRFS_IOC_SEND, &mut args);

        if let Err(ref e) = send_result {
            if e.kind() != ErrorKind::BrokenPipe {
                return send_result;
            }
        } else if let Some(handle) = recv_handle {
            let join_result = handle.join().unwrap();

            join_result?
        }

        send_result
    }
}
