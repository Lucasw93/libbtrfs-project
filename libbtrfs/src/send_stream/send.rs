use crate::{
    bindings::{btrfs_ioctl_send_args, BTRFS_IOC_SEND, BTRFS_SEND_FLAG_VERSION},
    fs::io::is_btrfs,
    util::btrfs_ioctl,
    util::send::supported_send_version,
    util::{IoError, IoResult},
    Flags,
};
use libc::F_SETPIPE_SZ;

use std::sync::{atomic::AtomicU64, Arc};

use std::{
    fs::File,
    io::{pipe, stdout, ErrorKind, PipeReader, PipeWriter},
    mem::MaybeUninit,
    os::fd::{AsFd, AsRawFd, OwnedFd},
    path::{Path, PathBuf},
    thread,
    thread::JoinHandle,
};

impl TryFrom<&Path> for SendBuilder
{
    type Error = IoError;

    fn try_from(value: &Path) -> Result<Self, Self::Error>
    {
        File::open(value).map(|f| Self::new_internal(f.into()))
    }
}

impl TryFrom<&str> for SendBuilder
{
    type Error = IoError;

    fn try_from(value: &str) -> Result<Self, Self::Error>
    {
        Self::try_from(Path::new(value))
    }
}

impl From<File> for SendBuilder
{
    fn from(value: File) -> Self
    {
        Self::new_internal(value.into())
    }
}

pub struct SendBuilder
{
    source: OwnedFd,
    receive: Option<PathBuf>,
    version: u32,
    send_fd: i64,
    flags: Flags,
}

impl SendBuilder
{
    const BUFSZ_V1: i32 = 64_000;
    const BUFSZ_V2: i32 = 128_000;

    fn new_internal(source: OwnedFd) -> Self
    {
        Self {
            source,
            receive: None,
            version: supported_send_version(),
            send_fd: stdout().as_raw_fd() as i64,
            flags: Flags::NONE,
        }
    }

    pub fn receive<P: AsRef<Path>>(mut self, destination: P) -> Self
    {
        self.receive = Some(destination.as_ref().to_path_buf());
        self
    }

    pub fn version(mut self, version: u32) -> Self
    {
        self.version = version;
        self
    }

    pub fn flags(mut self, flags: Flags) -> Self
    {
        self.flags = flags;
        self
    }

    pub fn spawn_send(mut self, progress: Arc<AtomicU64>) -> IoResult<JoinHandle<IoResult<()>>>
    {
        let (reader, writer): (PipeReader, PipeWriter);

        let recv_data = if let Some(dst) = self.receive {
            (reader, writer) = pipe()?;

            self.send_fd = writer.as_raw_fd() as i64;

            let pipe_sz = if self.version > 1 {
                Self::BUFSZ_V2
            } else {
                Self::BUFSZ_V1
            };
            syscall!(unsafe { fcntl(reader.as_raw_fd(), F_SETPIPE_SZ, pipe_sz) })?;

            Some((dst, reader, writer))
        } else {
            None
        };

        if !is_btrfs(self.source.as_fd())? {
            return Err(ErrorKind::Unsupported.into());
        }

        let flag_args = self.flags.to_raw_send_flags()?;

        let send_handle = thread::spawn(move || {
            let fd = self.source;
            let mut args: btrfs_ioctl_send_args = unsafe { MaybeUninit::zeroed().assume_init() };

            args.flags = flag_args;
            args.send_fd = self.send_fd;
            args.version = self.version;
            if args.version > 1 {
                args.flags |= BTRFS_SEND_FLAG_VERSION
            }

            btrfs_ioctl(fd.as_fd(), BTRFS_IOC_SEND, &mut args)
        });

        let _recv_handle = recv_data.map(|(dst, reader, writer)| {
            thread::spawn(move || {
                let _w = writer; // dont close the pipe

                super::receive_stream(dst, reader, Some(progress))
            })
        });

        Ok(send_handle)
    }

    pub fn send(mut self) -> IoResult<()>
    {
        let (reader, writer): (PipeReader, PipeWriter);

        let recv_handle = if let Some(dst) = self.receive {
            (reader, writer) = pipe()?;

            self.send_fd = writer.as_raw_fd() as i64;

            let pipe_sz = if self.version > 1 {
                Self::BUFSZ_V2
            } else {
                Self::BUFSZ_V1
            };
            syscall!(unsafe { fcntl(reader.as_raw_fd(), F_SETPIPE_SZ, pipe_sz) })?;

            thread::spawn(move || super::receive_stream(dst, reader, None)).into()
        } else {
            None
        };

        let mut args: btrfs_ioctl_send_args = unsafe { MaybeUninit::zeroed().assume_init() };

        args.flags = self.flags.to_raw_send_flags()?;
        args.send_fd = self.send_fd;
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
