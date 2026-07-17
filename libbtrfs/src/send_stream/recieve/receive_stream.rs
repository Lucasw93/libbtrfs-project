use crate::bindings::btrfs_ioctl_received_subvol_args;
use crate::{
    bindings::{BTRFS_IOC_SET_RECEIVED_SUBVOL, btrfs_ioctl_timespec},
    util::{IoResult, btrfs_ioctl},
};

use libc::{
    AT_FDCWD, AT_SYMLINK_NOFOLLOW, O_CREAT, O_TRUNC, O_WRONLY, PATH_MAX, S_IFIFO, S_IFMT,
    S_IFSOCK, S_IRUSR, S_IRWXU, S_IWUSR, c_char, c_void, dev_t, gid_t, mode_t, off_t, size_t,
    timespec, uid_t,
};
use std::{
    ffi::OsStr,
    fs::File,
    io::ErrorKind,
    os::{
        fd::{AsFd, FromRawFd, OwnedFd},
        unix::ffi::OsStrExt,
        unix::fs::OpenOptionsExt,
    },
    path::PathBuf,
};

use uuid::Uuid;

#[derive(Default)]
pub struct ReceiveStream
{
    pub destination: PathBuf,

    pub primary_path_buf: Vec<u8>,
    pub secondary_path_buf: Vec<u8>,

    pub base_path_len: usize,

    pub uuid: Uuid,
    pub stime: Option<btrfs_ioctl_timespec>,
    pub stransid: u64,
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

#[allow(unused)]
impl ReceiveStream
{
    pub(super) fn finish_send(&mut self) -> IoResult<Option<()>>
    {
        let mut args: btrfs_ioctl_received_subvol_args =
            unsafe { std::mem::MaybeUninit::zeroed().assume_init() };

        args.uuid = self.uuid.as_bytes().map(|b| b as i8);
        args.stransid = self.stransid;
        args.stime = self
            .stime
            .unwrap_or_else(get_btrfs_timespec_from_system_time);

        File::open(self.destination.as_path())
            .map(|f| btrfs_ioctl(f.as_fd(), BTRFS_IOC_SET_RECEIVED_SUBVOL, &mut args))?;

        Ok(None)
    }

    pub(super) fn subvol(&mut self, path: &[u8], uuid: Uuid, ctransid: u64)
    -> IoResult<Option<()>>
    {
        if !self.primary_path_buf.is_empty() {
            return receive_error!("data stream is invalid");
        }
        let subvol_dir = File::options()
            .read(true)
            .custom_flags(libc::O_DIRECTORY)
            .open(self.destination.as_path())?;

        // destination is now the path to the subvolume and can be used for `finish_send()`
        self.destination.push(OsStr::from_bytes(path));

        self.primary_path_buf = Vec::with_capacity(PATH_MAX as usize / 2);
        self.primary_path_buf
            .extend_from_slice(self.destination.as_os_str().as_bytes());
        self.primary_path_buf.push('/' as u8);

        self.base_path_len = self.primary_path_buf.len();

        self.secondary_path_buf = Vec::with_capacity(PATH_MAX as usize / 2);
        self.secondary_path_buf
            .extend_from_slice(&self.primary_path_buf[..]);

        self.uuid = uuid;
        self.stransid = ctransid;

        crate::subvol::io::create(subvol_dir, path)?;

        Ok(Some(()))
    }

    pub(super) fn mkfile(&mut self, path: *const u8, path_len: usize) -> IoResult<Option<()>>
    {
        let pathname = self.update_primary_path(path, path_len)?;
        let flags = O_WRONLY | O_CREAT | O_TRUNC;
        let mode = S_IRUSR | S_IWUSR;

        unsafe {
            OwnedFd::from_raw_fd(syscall!(open(pathname.cast::<c_char>(), flags, mode))?);
        }

        Ok(Some(()))
    }

    pub(super) fn mkdir(&mut self, path: *const u8, path_len: usize) -> IoResult<Option<()>>
    {
        let pathname = self.update_primary_path(path, path_len)?;

        syscall!(unsafe { mkdir(pathname.cast::<c_char>(), S_IRWXU) })?;

        Ok(Some(()))
    }

    pub(super) fn mknod(
        &mut self,
        path: *const u8,
        path_len: usize,
        mode: mode_t,
        dev: dev_t,
    ) -> IoResult<Option<()>>
    {
        let pathname = self.update_primary_path(path, path_len)?;

        syscall!(unsafe { mknod(pathname.cast::<c_char>(), mode & S_IFMT, dev) })?;

        Ok(Some(()))
    }

    pub(super) fn mkfifo(&mut self, path: *const u8, path_len: usize) -> IoResult<Option<()>>
    {
        let pathname = self.update_primary_path(path, path_len)?;
        let mode = S_IRUSR | S_IWUSR;

        syscall!(unsafe { mknod(pathname.cast::<c_char>(), mode | S_IFIFO, 0) })?;

        Ok(Some(()))
    }

    pub(super) fn mksock(&mut self, path: *const u8, path_len: usize) -> IoResult<Option<()>>
    {
        let pathname = self.update_primary_path(path, path_len)?;
        let mode = S_IRUSR | S_IWUSR;

        syscall!(unsafe { mknod(pathname.cast::<c_char>(), mode | S_IFSOCK, 0) })?;

        Ok(Some(()))
    }

    pub(super) fn symlink(
        &mut self,
        target: *const u8,
        target_len: usize,
        linkpath: *const u8,
        linkpath_len: usize,
    ) -> IoResult<Option<()>>
    {
        let full_target = self.update_primary_path(target, target_len)?;

        unsafe { linkpath.cast_mut().add(linkpath_len).write(0) }

        syscall!(unsafe { symlink(linkpath.cast::<c_char>(), full_target.cast::<c_char>()) })?;

        Ok(Some(()))
    }

    pub(super) fn rename(
        &mut self,
        from: *const u8,
        from_len: usize,
        to: *const u8,
        to_len: usize,
    ) -> IoResult<Option<()>>
    {
        let oldpath = self.update_primary_path(from, from_len)?;
        let newpath = self.update_secondary_path(to, to_len)?;

        syscall!(unsafe { rename(oldpath.cast::<c_char>(), newpath.cast::<c_char>()) })?;

        Ok(Some(()))
    }

    pub(super) fn link(
        &mut self,
        path: *const u8,
        path_len: usize,
        link: *const u8,
        link_len: usize,
    ) -> IoResult<Option<()>>
    {
        let oldpath = self.update_primary_path(path, path_len)?;
        let newpath = self.update_secondary_path(link, link_len)?;

        syscall!(unsafe { link(newpath.cast::<c_char>(), oldpath.cast::<c_char>()) })?;

        Ok(Some(()))
    }

    pub(super) fn unlink(&mut self, path: *const u8, path_len: usize) -> IoResult<Option<()>>
    {
        let pathname = self.update_primary_path(path, path_len)?;

        syscall!(unsafe { unlink(pathname.cast::<c_char>()) })?;

        Ok(Some(()))
    }

    pub(super) fn rmdir(&mut self, path: *const u8, path_len: usize) -> IoResult<Option<()>>
    {
        let pathname = self.update_primary_path(path, path_len)?;

        syscall!(unsafe { rmdir(pathname.cast::<c_char>()) })?;

        Ok(Some(()))
    }

    pub(super) fn write(
        &mut self,
        path: *const u8,
        path_len: usize,
        buf: *const u8,
        count: size_t,
        offset: off_t,
    ) -> IoResult<Option<()>>
    {
        let pathname = self.update_primary_path(path, path_len)?;
        let fd = syscall!(unsafe { open(pathname.cast::<c_char>(), O_WRONLY) })?;
        let _owned = unsafe { OwnedFd::from_raw_fd(fd) };

        let mut pos: usize = 0;
        while pos < count {
            match syscall!(unsafe {
                pwrite(fd, buf.add(pos).cast::<c_void>(), count - pos, offset)
            }) {
                Ok(rbytes) => pos += rbytes as usize,
                Err(e) => return Err(e),
            }
        }

        Ok(Some(()))
    }

    pub(super) fn set_xattr(
        &mut self,
        path: *const u8,
        path_len: usize,
        xattr_name: *const u8,
        xattr_name_len: usize,
        data: *const u8,
        data_len: usize,
    ) -> IoResult<Option<()>>
    {
        let pathname = self.update_primary_path(path, path_len)?;
        unsafe { xattr_name.cast_mut().add(xattr_name_len).write(0) }

        syscall!(unsafe {
            lsetxattr(
                pathname.cast::<c_char>(),
                xattr_name.cast::<c_char>(),
                data.cast::<c_void>(),
                data_len,
                0,
            )
        })?;

        Ok(Some(()))
    }

    pub(super) fn truncate(
        &mut self,
        path: *const u8,
        path_len: usize,
        length: off_t,
    ) -> IoResult<Option<()>>
    {
        let path = self.update_primary_path(path, path_len)?;

        syscall!(unsafe { truncate(path.cast::<c_char>(), length) })?;

        Ok(Some(()))
    }

    pub(super) fn clone()
    {
        unimplemented!()
    }

    pub(super) fn remove_xattr()
    {
        unimplemented!()
    }

    pub(super) fn chmod(
        &mut self,
        path: *const u8,
        path_len: usize,
        mode: mode_t,
    ) -> IoResult<Option<()>>
    {
        let pathname = self.update_primary_path(path, path_len)?;

        syscall!(unsafe { chmod(pathname.cast::<c_char>(), mode) })?;

        Ok(Some(()))
    }

    pub(super) fn chown(
        &mut self,
        path: *const u8,
        path_len: usize,
        uid: uid_t,
        gid: gid_t,
    ) -> IoResult<Option<()>>
    {
        let pathname = self.update_primary_path(path, path_len)?;

        syscall!(unsafe { lchown(pathname.cast::<c_char>(), uid, gid) })?;

        Ok(Some(()))
    }

    pub(super) fn utimes(
        &mut self,
        path: *const u8,
        path_len: usize,
        atime: timespec,
        mtime: timespec,
    ) -> IoResult<Option<()>>
    {
        let pathname = self.update_primary_path(path, path_len)?;
        let times = [atime, mtime].as_ptr();

        self.stime = Some(btrfs_ioctl_timespec {
            sec: atime.tv_sec as u64,
            nsec: atime.tv_nsec as u32,
        });

        syscall!(unsafe {
            utimensat(
                AT_FDCWD,
                pathname.cast::<c_char>(),
                times,
                AT_SYMLINK_NOFOLLOW,
            )
        })?;

        Ok(Some(()))
    }

    pub(super) fn update_extent(
        &mut self,
        path: &[u8],
        offset: u64,
        len: u64,
    ) -> IoResult<Option<()>>
    {
        // eprintln!("update_extent {path}, offset={offset}, len={len}");

        /*
         * Sent with SendFlag::NO_FILE_DATA, nothing to do.
         */
        Ok(Some(()))
    }

    pub(super) fn enceded_write()
    {
        unimplemented!()
    }

    pub(super) fn fallocate()
    {
        unimplemented!()
    }

    #[inline(always)]
    fn update_primary_path(&mut self, path_ptr: *const u8, len: usize) -> IoResult<*const u8>
    {
        let total_len = self.base_path_len + len;

        if total_len > self.primary_path_buf.capacity() {
            if total_len >= PATH_MAX as usize {
                return Err(ErrorKind::InvalidFilename.into());
            }
            self.primary_path_buf
                .resize_with(total_len, Default::default);
        }

        let base_path = self.primary_path_buf.as_mut_ptr();
        unsafe {
            let p = base_path.add(self.base_path_len);
            path_ptr.copy_to_nonoverlapping(p, len);
            p.add(len).write(0)
        }
        Ok(base_path)
    }

    #[inline(always)]
    fn update_secondary_path(&mut self, path_ptr: *const u8, len: usize) -> IoResult<*const u8>
    {
        let total_len = self.base_path_len + len;

        if total_len > self.secondary_path_buf.capacity() {
            if total_len >= PATH_MAX as usize {
                return Err(ErrorKind::InvalidFilename.into());
            }
            self.secondary_path_buf
                .resize_with(total_len, Default::default);
        }

        let base_path = self.secondary_path_buf.as_mut_ptr();
        unsafe {
            let p = base_path.add(self.base_path_len);
            path_ptr.copy_to_nonoverlapping(p, len);
            p.add(len).write(0);
        }
        Ok(base_path)
    }
}
