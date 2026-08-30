use super::*;
use crate::{
    bindings::btrfs_ioctl_received_subvol_args,
    bindings::{BTRFS_IOC_SET_RECEIVED_SUBVOL, btrfs_ioctl_timespec},
    util::btrfs_ioctl,
};
use std::{
    fs::File,
    io::{self, ErrorKind},
    mem::{self, ManuallyDrop, MaybeUninit},
    os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd, RawFd},
    path::Path,
};
use uuid::Uuid;

struct XattrFallback
{
    buf: Vec<u8>,
    len: usize,
}

impl From<RawFd> for XattrFallback
{
    fn from(dfd: RawFd) -> Self
    {
        use std::io::Write;

        let mut buf = Vec::with_capacity(256);
        write!(&mut buf, "/proc/self/fd/{}/", dfd).unwrap();

        Self { len: buf.len(), buf }
    }
}

impl From<&Path> for XattrFallback
{
    fn from(path: &Path) -> Self
    {
        use std::os::unix::ffi::OsStrExt;

        let mut buf = Vec::with_capacity(256);
        buf.extend_from_slice(path.as_os_str().as_bytes());
        if buf.last() != Some(&b'/') {
            buf.push(b'/')
        }
        Self { len: buf.len(), buf }
    }
}

/// Handles a full BTRFS send stream.
pub struct HandleFull<R>
{
    dirfd: R,
    path_buf: Box<[u8; libc::PATH_MAX as _]>,
    path_buf_secondary: Box<[u8; libc::PATH_MAX as _]>,
    current_subvol: Box<[u8]>,
    received_args: btrfs_ioctl_received_subvol_args,

    // Fallback for Linux < 6.13 where `setxattrat()` and `removexattrat()` is not supported
    __xattr_fallback: XattrFallback,
}

impl HandleFull<()>
{
    // defined in <linux/limit.h>
    const XATTR_NAME_MAX: usize = 255;
    const MODE: libc::mode_t = libc::S_IRUSR | libc::S_IWUSR;
}

impl HandleFull<OwnedFd>
{
    /// Constructs a new [`HandleFull`] instance from a path.
    ///
    /// This functions constructs a [`HandleFull`] instance that will receive and handle send
    /// commands at the mount point provided by `path`. `path` must exist and refer to a directory.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::NotADirectory`]
    ///
    /// > `path` does not refer to directory.
    ///
    /// [`ErrorKind::NotFound`]
    ///
    /// > No file or directory exists at `path`.
    pub fn from_path<P: AsRef<Path>>(path: P) -> std::io::Result<Self>
    {
        let dirfd = File::open(path.as_ref())?;

        if !dirfd.metadata()?.is_dir() {
            return Err(ErrorKind::NotADirectory.into());
        }

        Ok(Self {
            dirfd: OwnedFd::from(dirfd),
            current_subvol: Box::default(),
            path_buf: unsafe { Box::new_zeroed().assume_init() },
            path_buf_secondary: unsafe { Box::new_zeroed().assume_init() },
            received_args: unsafe { MaybeUninit::zeroed().assume_init() },

            __xattr_fallback: XattrFallback::from(path.as_ref()),
        })
    }
}

impl<R: AsFd> HandleFull<R>
{
    /// Constructs a new [`HandleFull`] instance from a file descriptor.
    ///
    /// This functions constructs a [`HandleFull`] instance that will receive and handle send
    /// commands at the mount point provided by `dirfd`. `dirfd` must refer to a directory.
    ///
    /// # Notes
    ///
    /// In this implantation, the `setxattrat` and `removexattrat` system calls are used to handle
    /// the `SetXattrCmd`, and `RemoveXattrCmd` commands. These system calls were introduced in
    /// linux < 6.13. For systems that do not support `setxattrat` and `removexattrat`, a fallback
    /// using `lsetxattr` with the path arguement, `/proc/self/fd/<dirfd>/` is used. For containers
    /// and chroot enviroments, it is recommeneded to use the [`HandleFull::from_path`] constructor
    /// instead, which will call `lsetxattr` using the provided `path` argument, since looking up
    /// the mountpoint using the proc filesystem may fail in these situations.    
    ///
    /// # Errors
    ///
    /// [`ErrorKind::NotADirectory`]
    ///
    /// > `path` does not refer to directory.
    pub fn new(dirfd: R) -> std::io::Result<Self>
    {
        let f = ManuallyDrop::new(unsafe { File::from_raw_fd(dirfd.as_fd().as_raw_fd()) });

        if !f.metadata()?.is_dir() {
            return Err(ErrorKind::NotADirectory.into());
        }

        Ok(Self {
            __xattr_fallback: XattrFallback::from(dirfd.as_fd().as_raw_fd()),

            dirfd: dirfd,
            current_subvol: Box::default(),
            path_buf: unsafe { Box::new_zeroed().assume_init() },
            path_buf_secondary: unsafe { Box::new_zeroed().assume_init() },
            received_args: unsafe { MaybeUninit::zeroed().assume_init() },
        })
    }

    fn uuid_is_current(&self, uuid: Uuid) -> bool
    {
        let current = self.received_args.uuid;
        uuid.as_bytes()
            .iter()
            .enumerate()
            .all(|(i, &b)| b == current[i] as u8)
    }

    fn get_btrfs_timespec_from_system_time() -> btrfs_ioctl_timespec
    {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();

        btrfs_ioctl_timespec {
            sec: now.as_secs(),
            nsec: now.as_secs() as u32,
        }
    }

    #[inline(always)]
    fn get_c_path(&mut self, path: &[u8]) -> std::io::Result<*const libc::c_char>
    {
        Self::do_c_path(path, &mut self.path_buf, self.current_subvol.len())
    }

    #[inline(always)]
    fn get_c_path_secondary(&mut self, path: &[u8]) -> std::io::Result<*const libc::c_char>
    {
        Self::do_c_path(
            path,
            &mut self.path_buf_secondary,
            self.current_subvol.len(),
        )
    }

    #[inline(always)]
    fn do_c_path(
        path: &[u8],
        path_buf: &mut [u8; libc::PATH_MAX as _],
        start: usize,
    ) -> std::io::Result<*const libc::c_char>
    {
        if let Some(slice) = path_buf.get_mut(start..=(start + path.len())) {
            slice[..path.len()].copy_from_slice(path);
            slice[path.len()] = 0;

            Ok(path_buf.as_ptr().cast())
        } else {
            Err(std::io::Error::from_raw_os_error(libc::ENAMETOOLONG))
        }
    }

    #[inline(always)]
    fn sys_openat_secondary(
        &mut self,
        path: &[u8],
        flags: libc::c_int,
        mode: Option<u32>,
    ) -> std::io::Result<OwnedFd>
    {
        Self::do_sys_openat(
            self.as_raw_fd(),
            self.get_c_path_secondary(path)?,
            flags,
            mode,
        )
    }

    #[inline(always)]
    fn sys_openat(
        &mut self,
        path: &[u8],
        flags: libc::c_int,
        mode: Option<u32>,
    ) -> std::io::Result<OwnedFd>
    {
        Self::do_sys_openat(self.as_raw_fd(), self.get_c_path(path)?, flags, mode)
    }

    fn do_sys_openat(
        raw_fd: std::os::fd::RawFd,
        path_ptr: *const libc::c_char,
        flags: libc::c_int,
        mode: Option<u32>,
    ) -> std::io::Result<OwnedFd>
    {
        unsafe {
            match mode {
                Some(mode) => syscall!(openat(raw_fd, path_ptr, flags, mode)),
                None => syscall!(openat(raw_fd, path_ptr, flags)),
            }
            .map(|raw| OwnedFd::from_raw_fd(raw))
        }
    }
}

impl<R: AsFd> AsRawFd for HandleFull<R>
{
    fn as_raw_fd(&self) -> RawFd
    {
        self.dirfd.as_fd().as_raw_fd()
    }
}

impl<R: AsFd + Send> StreamHandler for HandleFull<R>
{
    fn subvol(&mut self, SubvolCmd { path, uuid, ctransid }: SubvolCmd) -> io::Result<Option<()>>
    {
        if !self.current_subvol.is_empty() {
            return receive_error!("Stream is invalid");
        }
        let mut subvol = path.to_vec();

        if subvol.last() != Some(&b'/') {
            subvol.push(b'/');
        }
        self.path_buf[..subvol.len()].copy_from_slice(&subvol);
        self.path_buf_secondary[..subvol.len()].copy_from_slice(&subvol);
        self.current_subvol = subvol.into_boxed_slice();

        self.received_args.stransid = ctransid;
        self.received_args.uuid = uuid.as_bytes().map(|b| b as i8);
        self.received_args.stime = Self::get_btrfs_timespec_from_system_time();

        crate::subvol::fd::create(self.dirfd.as_fd(), path)?;

        Ok(Some(()))
    }

    fn snapshot(
        &mut self,
        SnapshotCmd {
            path,
            uuid,
            clone_uuid,
            ctransid,
            clone_ctransid,
        }: SnapshotCmd,
    ) -> io::Result<Option<()>>
    {
        unimplemented!(
            concat!(
                " - BTRFS_SEND_C_SNAPSHOT (2) - \n",
                " path: {}\n uuid: {}\n clone_uuid: {}\n ctransid: {}\n clone_ctransid: {}",
            ),
            String::from_utf8_lossy(path),
            uuid,
            clone_uuid,
            ctransid,
            clone_ctransid,
        )
    }

    fn mkfile(&mut self, MkfileCmd { path, ino: _ }: MkfileCmd) -> io::Result<Option<()>>
    {
        let flags = libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC;

        self.sys_openat(path, flags, Some(HandleFull::MODE))?;

        Ok(Some(()))
    }

    fn mkdir(&mut self, MkdirCmd { path, ino: _ }: MkdirCmd) -> io::Result<Option<()>>
    {
        let pathname = self.get_c_path(path)?;

        syscall!(unsafe { mkdirat(self.as_raw_fd(), pathname, libc::S_IRWXU) })?;

        Ok(Some(()))
    }

    fn mknod(&mut self, MknodCmd { path, mode, rdev }: MknodCmd) -> io::Result<Option<()>>
    {
        let pathname = self.get_c_path(path)?;
        let mode = mode as libc::mode_t & libc::S_IFMT;
        let dev = rdev as libc::dev_t;

        syscall!(unsafe { mknodat(self.as_raw_fd(), pathname, mode, dev) })?;

        Ok(Some(()))
    }

    fn mkfifo(&mut self, MkfifoCmd { path, ino: _ }: MkfifoCmd) -> io::Result<Option<()>>
    {
        let pathname = self.get_c_path(path)?;
        let mode = HandleFull::MODE | libc::S_IFIFO;

        syscall!(unsafe { mknodat(self.as_raw_fd(), pathname, mode, 0) })?;

        Ok(Some(()))
    }

    fn mksock(&mut self, MksockCmd { path, ino: _ }: MksockCmd) -> io::Result<Option<()>>
    {
        let pathname = self.get_c_path(path)?;
        let mode = HandleFull::MODE | libc::S_IFSOCK;

        syscall!(unsafe { mknodat(self.as_raw_fd(), pathname, mode, 0) })?;

        Ok(Some(()))
    }

    fn symlink(
        &mut self,
        SymlinkCmd { path, ino: _, path_link }: SymlinkCmd,
    ) -> io::Result<Option<()>>
    {
        let linkpath = self.get_c_path(path)?;
        let target = self.get_c_path_secondary(path_link)?;

        syscall!(unsafe { symlinkat(target, self.as_raw_fd(), linkpath) })?;

        Ok(Some(()))
    }

    fn rename(&mut self, RenameCmd { path, path_to }: RenameCmd) -> io::Result<Option<()>>
    {
        let oldpath = self.get_c_path(path)?;
        let newpath = self.get_c_path_secondary(path_to)?;

        syscall!(unsafe { renameat2(self.as_raw_fd(), oldpath, self.as_raw_fd(), newpath, 0) })?;

        Ok(Some(()))
    }

    fn link(&mut self, LinkCmd { path, path_link }: LinkCmd) -> io::Result<Option<()>>
    {
        let oldpath = self.get_c_path(path)?;
        let newpath = self.get_c_path_secondary(path_link)?;

        syscall!(unsafe { linkat(self.as_raw_fd(), newpath, self.as_raw_fd(), oldpath, 0) })?;

        Ok(Some(()))
    }

    fn unlink(&mut self, UnlinkCmd { path }: UnlinkCmd) -> io::Result<Option<()>>
    {
        let pathname = self.get_c_path(path)?;

        syscall!(unsafe { unlinkat(self.as_raw_fd(), pathname, 0) })?;

        Ok(Some(()))
    }

    fn rmdir(&mut self, RmdirCmd { path }: RmdirCmd) -> io::Result<Option<()>>
    {
        let pathname = self.get_c_path(path)?;

        syscall!(unsafe { unlinkat(self.as_raw_fd(), pathname, libc::AT_REMOVEDIR) })?;

        Ok(Some(()))
    }

    fn set_xattr(
        &mut self,
        SetXattrCmd { path, xattr_name, xattr_data }: SetXattrCmd,
    ) -> io::Result<Option<()>>
    {
        // NOTE: The `setxattrat()` system call was only introducted in Linux 6.13 [1].
        // On most platforms it is syscall number 463 [2].
        // It will fail with errno set to `ENOSYS` on linux < 6.13.
        //
        // A workaround to use a `dirfd` with the `setxattr()` system call is the use
        // "/proc/self/fd/<dirfd>/<path>", as the path argument, but this is not ideal because it will
        // fail in chroot enviroments and containers.
        //
        // [1] https://lwn.net/Articles/998623/
        // [2] https://docs.rs/syscalls/latest/syscalls/x86_64/enum.Sysno.html#variant.setxattrat
        fn fallback(
            path: &[u8],
            name_ptr: *const u8,
            xattr_data: &[u8],
            subvol: &[u8],
            fallback: &mut XattrFallback,
        ) -> io::Result<Option<()>>
        {
            fallback.buf.truncate(fallback.len);
            fallback.buf.extend_from_slice(subvol);
            fallback.buf.extend_from_slice(path);
            fallback.buf.push(0);
            syscall!(unsafe {
                lsetxattr(
                    fallback.buf.as_ptr().cast(),
                    name_ptr.cast::<libc::c_char>(),
                    xattr_data.as_ptr().cast::<libc::c_void>(),
                    xattr_data.len(),
                    0,
                )
            })?;

            Ok(Some(()))
        }

        // From <linux/xattr.h> (in linux >= 6.13)
        #[repr(C)]
        #[repr(align(8))]
        struct XattrArgs
        {
            value: u64,
            size: u32,
            flags: u32,
        }

        #[cfg(not(any(target_arch = "mips", target_arch = "mips64")))]
        const SYSNO: libc::c_long = 463;
        #[cfg(target_arch = "mips")]
        const SYSNO: libc::c_long = 4_463;
        #[cfg(target_arch = "mips64")]
        const SYSNO: libc::c_long = 5_463;

        if xattr_name.len() > HandleFull::XATTR_NAME_MAX {
            return receive_error!("xattr name too long");
        }
        let mut xattr_name_buf = [0u8; HandleFull::XATTR_NAME_MAX + 1];
        let path_ptr = self.get_c_path(path)?;
        let args = XattrArgs {
            value: xattr_data.as_ptr() as u64,
            size: xattr_data.len() as u32,
            flags: 0,
        };

        xattr_name_buf[..xattr_name.len()].copy_from_slice(xattr_name);
        let res = unsafe {
            libc::syscall(
                SYSNO,
                path_ptr.cast::<libc::c_char>(),
                libc::AT_SYMLINK_NOFOLLOW,
                xattr_name_buf.as_ptr().cast::<libc::c_char>(),
                &args,
                size_of_val(&args),
            )
        };
        if res == -1 {
            let err = std::io::Error::last_os_error();

            if let Some(libc::ENOSYS) = err.raw_os_error() {
                return fallback(
                    path,
                    xattr_name_buf.as_ptr(),
                    xattr_data,
                    &self.current_subvol,
                    &mut self.__xattr_fallback,
                );
            } else {
                return Err(err);
            }
        }

        Ok(Some(()))
    }

    fn remove_xattr(
        &mut self,
        RemoveXattrCmd { path, xattr_name }: RemoveXattrCmd,
    ) -> io::Result<Option<()>>
    {
        // NOTE: The `removexattrat()` system call was only introducted in Linux 6.13 [1]
        // On most platforms it is syscall number 466 [2].
        // It will fail with errno set to `ENOSYS` on linux < 6.13.
        //
        // A workaround to use a `dirfd` with the `removexattr()` system call is the use
        // "/proc/self/fd/<dirfd>/<path>", as the path argument, but this is not ideal because it will
        // fail in chroot enviroments and containers.
        //
        // [1] https://lwn.net/Articles/998623/
        // [2] https://docs.rs/syscalls/latest/syscalls/x86_64/enum.Sysno.html#variant.removexattrat
        fn fallback(
            path: &[u8],
            name_ptr: *const u8,
            subvol: &[u8],
            fallback: &mut XattrFallback,
        ) -> io::Result<Option<()>>
        {
            fallback.buf.truncate(fallback.len);
            fallback.buf.extend_from_slice(subvol);
            fallback.buf.extend_from_slice(path);
            fallback.buf.push(0);
            syscall!(unsafe {
                lremovexattr(
                    fallback.buf.as_ptr().cast(),
                    name_ptr.cast::<libc::c_char>(),
                )
            })?;

            Ok(Some(()))
        }
        #[cfg(not(any(target_arch = "mips", target_arch = "mips64")))]
        const SYSNO: libc::c_long = 466;
        #[cfg(target_arch = "mips")]
        const SYSNO: libc::c_long = 4_466;
        #[cfg(target_arch = "mips64")]
        const SYSNO: libc::c_long = 5_466;

        if xattr_name.len() > HandleFull::XATTR_NAME_MAX {
            return receive_error!("xattr name too long");
        }
        let mut xattr_name_buf = [0u8; HandleFull::XATTR_NAME_MAX + 1];
        let path_ptr = self.get_c_path(path)?;

        xattr_name_buf[..xattr_name.len()].copy_from_slice(xattr_name);
        let res = unsafe {
            libc::syscall(
                SYSNO,
                path_ptr.cast::<libc::c_char>(),
                libc::AT_SYMLINK_NOFOLLOW,
                xattr_name_buf.as_ptr().cast::<libc::c_char>(),
            )
        };
        if res == -1 {
            let err = std::io::Error::last_os_error();

            if let Some(libc::ENOSYS) = err.raw_os_error() {
                return fallback(
                    path,
                    xattr_name_buf.as_ptr(),
                    &self.current_subvol,
                    &mut self.__xattr_fallback,
                );
            } else {
                return Err(err);
            }
        }

        Ok(Some(()))
    }

    fn write(&mut self, WriteCmd { path, file_offset, data }: WriteCmd) -> io::Result<Option<()>>
    {
        let fd = self.sys_openat(path, libc::O_WRONLY, None)?;

        let mut pos: usize = 0;
        while pos < data.len() {
            match syscall!(unsafe {
                pwrite(
                    fd.as_raw_fd(),
                    data.as_ptr().add(pos).cast::<libc::c_void>(),
                    data.len() - pos,
                    file_offset as libc::off_t,
                )
            }) {
                Ok(rbytes) => pos += rbytes as usize,
                Err(e) => return Err(e),
            }
        }

        Ok(Some(()))
    }

    fn clone(
        &mut self,
        CloneCmd {
            path,
            file_offset,
            clone_len,
            clone_uuid,
            clone_ctransid,
            clone_path,
            clone_offset,
        }: CloneCmd,
    ) -> io::Result<Option<()>>
    {
        // destination
        let dest_fd =
            self.sys_openat(path, libc::O_WRONLY | libc::O_CREAT, Some(HandleFull::MODE))?;

        if !self.uuid_is_current(clone_uuid) {
            unimplemented!(
                concat!(
                    "\n - CLONE COMMAND WITH INCREMENTAL SEND - \n",
                    " path: {}\n clone_path: {}\n clone_uuid: {:?}\n clone_ctransid: {:?}",
                ),
                String::from_utf8_lossy(path),
                String::from_utf8_lossy(clone_path),
                clone_uuid,
                clone_ctransid,
            )
        }

        // src/clone
        let clone_fd = self.sys_openat_secondary(clone_path, libc::O_RDONLY, None)?;

        let mut args: crate::bindings::btrfs_ioctl_clone_range_args =
            unsafe { MaybeUninit::zeroed().assume_init() };

        args.src_fd = clone_fd.as_raw_fd() as _;
        args.src_length = clone_len;
        args.src_offset = clone_offset;
        args.dest_offset = file_offset;
        btrfs_ioctl(dest_fd, crate::bindings::BTRFS_IOC_CLONE_RANGE, &mut args)?;

        Ok(Some(()))
    }

    fn truncate(&mut self, TruncateCmd { path, size }: TruncateCmd) -> io::Result<Option<()>>
    {
        if size == 0 {
            self.sys_openat(path, libc::O_TRUNC | libc::O_WRONLY, None)?;
        } else {
            let fd = self.sys_openat(path, libc::O_WRONLY, None)?;

            syscall!(unsafe { ftruncate(fd.as_raw_fd(), size as libc::off_t) })?;
        };

        Ok(Some(()))
    }

    fn chmod(&mut self, ChmodCmd { path, mode }: ChmodCmd) -> io::Result<Option<()>>
    {
        let pathname = self.get_c_path(path)?;

        syscall!(unsafe {
            fchmodat(
                self.as_raw_fd(),
                pathname,
                mode as libc::mode_t,
                libc::AT_SYMLINK_NOFOLLOW,
            )
        })?;

        Ok(Some(()))
    }

    fn chown(&mut self, ChownCmd { path, uid, gid }: ChownCmd) -> io::Result<Option<()>>
    {
        let pathname = self.get_c_path(path)?;
        let owner = uid as libc::uid_t;
        let group = gid as libc::gid_t;

        syscall!(unsafe {
            fchownat(
                self.as_raw_fd(),
                pathname,
                owner,
                group,
                libc::AT_SYMLINK_NOFOLLOW,
            )
        })?;

        Ok(Some(()))
    }

    fn utimes(
        &mut self,
        UtimesCmd { path, atime, mtime, ctime: _ }: UtimesCmd,
    ) -> io::Result<Option<()>>
    {
        let pathname = self.get_c_path(path)?;

        // NOTE: dont call `as_ptr()` here. It causes problems with release builds.
        let times = [atime, mtime];

        syscall!(unsafe {
            utimensat(
                self.as_raw_fd(),
                pathname,
                times.as_ptr().cast::<libc::timespec>(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        })?;

        Ok(Some(()))
    }

    fn end(&mut self, EndCmd: EndCmd) -> io::Result<Option<()>>
    {
        if self.current_subvol.is_empty() {
            return receive_error!("Invalid stream");
        }
        let subvol_path = mem::take(&mut self.current_subvol);

        let fd = self.sys_openat(&subvol_path, libc::O_RDONLY, None)?;

        btrfs_ioctl(fd, BTRFS_IOC_SET_RECEIVED_SUBVOL, &mut self.received_args)?;
        unsafe {
            std::ptr::write_bytes(&mut self.received_args, 0, 1);
        }
        Ok(None)
    }

    fn update_extent(
        &mut self,
        UpdateExtentCmd { path: _, file_offset: _, size: _ }: UpdateExtentCmd,
    ) -> io::Result<Option<()>>
    {
        // eprintln!("update_extent {path}, offset={offset}, len={len}");

        // Sent with FLAG::NO_FILE_DATA
        Ok(Some(()))
    }

    // =================================================================
    // Version 2
    // =================================================================

    fn fallocate(
        &mut self,
        FallocateCmd { path, fallocate_mode, file_offset, size }: FallocateCmd,
    ) -> io::Result<Option<()>>
    {
        let fd = self.sys_openat(path, libc::O_WRONLY, None)?;

        syscall!(unsafe {
            fallocate(
                fd.as_raw_fd(),
                fallocate_mode as libc::c_int,
                file_offset as libc::off_t,
                size as libc::off_t,
            )
        })?;

        Ok(Some(()))
    }

    fn fileattr(&mut self, FileattrCmd { path, fileattr }: FileattrCmd) -> io::Result<Option<()>>
    {
        eprintln!(
            "FileattrCmd: path: {}, fileattr: {fileattr}",
            String::from_utf8_lossy(path)
        );

        Ok(Some(()))
    }

    fn encoded_write(
        &mut self,
        EncodedWriteCmd {
            path,
            file_offset,
            unencoded_file_len,
            unencoded_len,
            unencoded_offset,
            compression,
            encryption,
            data,
        }: EncodedWriteCmd,
    ) -> io::Result<Option<()>>
    {
        let fd = self.sys_openat(path, libc::O_WRONLY, None)?;

        let mut args: crate::bindings::btrfs_ioctl_encoded_io_args =
            unsafe { MaybeUninit::zeroed().assume_init() };

        let mut iov = [libc::iovec {
            // SAFETY: BTRFS_IOC_ENCODED_WRITE is an _IOW ioctl, (read from usersapce - write parameters).
            // This means the memory in `data` will not be written to.
            iov_base: data.as_ptr().cast_mut().cast(),
            iov_len: data.len(),
        }];
        args.iov = iov.as_mut_ptr().cast();
        args.iovcnt = iov.len() as u64;
        args.offset = file_offset as i64;
        args.len = unencoded_file_len;
        args.unencoded_len = unencoded_len;
        args.unencoded_offset = unencoded_offset;
        args.compression = compression.unwrap_or_default();
        args.encryption = encryption.unwrap_or_default();
        btrfs_ioctl(fd, crate::bindings::BTRFS_IOC_ENCODED_WRITE, &mut args)?;

        Ok(Some(()))
    }
}
