use super::*;
use crate::ffi::UnixPath;
use uuid::Uuid;

/// Information about a btrfs device
///
/// Constructed by calling [`info()`], or the iterator returned by [`iter()`].
#[repr(transparent)]
pub struct DevInfo(btrfs_ioctl_dev_info_args);

impl DevInfo
{
    /// Id for this device
    pub fn device_id(&self) -> u64
    {
        self.0.devid
    }

    /// Uuid for this device
    pub fn device_uuid(&self) -> Uuid
    {
        Uuid::from_bytes(self.0.uuid)
    }

    /// Used bytes for this device
    pub fn bytes_used(&self) -> u64
    {
        self.0.bytes_used
    }

    /// Total bytes for this device
    pub fn bytes_total(&self) -> u64
    {
        self.0.total_bytes
    }

    /// Path for this device
    pub fn path(&self) -> &UnixPath
    {
        unsafe { UnixPath::from_ptr(self.0.path.as_ptr().cast()) }
    }

    /// Filesystem uuid
    ///
    /// # Notes
    ///
    /// This function requires kernel >= v6.3
    pub fn fsid(&self) -> Uuid
    {
        Uuid::from_bytes(self.0.fsid)
    }
}

/// Information about a device in a btrfs filesystem
///
/// Returns information about a device in a btrfs filesystem with the device id of `devid`
///
/// For an iterator over all devices in a btrfs filesystem see [`iter()`].
pub fn info<P: AsRef<Path>>(devid: u64, fs: P) -> IoResult<DevInfo>
{
    File::open(fs).and_then(|f| io::info(devid, f))
}

/// Returns a [`DevInfo`] struct that has been allocated on the heap.
pub fn boxed_info<P: AsRef<Path>>(devid: u64, fs: P) -> IoResult<Box<DevInfo>>
{
    File::open(fs).and_then(|f| io::boxed_info(devid, f))
}

/// Returns an iterator over devices in a btrfs filesystem
///
/// Call to next yeild instances of <code>[std::io::Result]<[DevInfo]></code>
///
/// # Errors
///
/// [`ErrorKind::InvalidData`]
///
/// > The iterator encounted an unexpeced number of devices
///
/// # Examples
///
/// ```no_run
/// for dev in libbtrfs::dev::iter("/")? {
///     let dev = dev?;
///     println!("device id: {}", dev.device_id());
///     println!(
///         "bytes-used: {:.2}GiB",
///         dev.bytes_used() as f32 / 2f32.powi(30)
///     );
///     println!(
///         "bytes-total: {:.2}GiB",
///         dev.bytes_total() as f32 / 2f32.powi(30)
///     );
/// }
///
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn iter<P: AsRef<Path>>(
    fs: P,
) -> IoResult<impl Iterator<Item = IoResult<DevInfo>> + ExactSizeIterator>
{
    File::open(fs).and_then(|f| Iter::new_internal(f))
}

pub(super) mod io
{
    use super::*;
    use std::os::fd::AsFd;

    /// See [super::info()]
    pub fn info<R: AsFd>(devid: u64, resource: R) -> IoResult<DevInfo>
    {
        let mut info_args: DevInfo = unsafe { MaybeUninit::zeroed().assume_init() };

        info_args.0.devid = devid;

        btrfs_ioctl(resource, BTRFS_IOC_DEV_INFO, &mut info_args).map(|_| info_args)
    }

    /// See [super::boxed_info()]
    pub fn boxed_info<R: AsFd>(devid: u64, resource: R) -> IoResult<Box<DevInfo>>
    {
        let mut info_args: Box<DevInfo> = unsafe { Box::new_zeroed().assume_init() };

        info_args.0.devid = devid;

        btrfs_ioctl(resource, BTRFS_IOC_DEV_INFO, info_args.as_mut()).map(|_| info_args)
    }

    /// See [super::iter()]
    pub fn iter<R: AsFd>(
        fd: R,
    ) -> IoResult<impl Iterator<Item = IoResult<DevInfo>> + ExactSizeIterator>
    {
        Iter::new_internal(fd)
    }
}

struct Iter<R: AsFd>
{
    fs: R,
    num_devices: u64,
    max_id: u64,
    found_devices: u64,
    last_id: u64,
}

impl<R: AsFd> Iter<R>
{
    fn new_internal(fs: R) -> IoResult<Self>
    {
        crate::fs::io::info(fs.as_fd(), crate::Flags::NONE).map(|fs_info| Iter {
            fs,
            max_id: fs_info.max_id(),
            num_devices: fs_info.num_devices(),
            found_devices: 0,
            last_id: 0,
        })
    }
}

impl<R: AsFd> ExactSizeIterator for Iter<R> {}

impl<R: AsFd> Iterator for Iter<R>
{
    type Item = IoResult<DevInfo>;

    fn next(&mut self) -> Option<Self::Item>
    {
        if self.last_id >= self.max_id {
            return if self.found_devices != self.num_devices {
                Some(Err(ErrorKind::InvalidData.into()))
            } else {
                None
            };
        }
        let mut args: DevInfo = unsafe { MaybeUninit::zeroed().assume_init() };

        for id in 1 + self.last_id.. {
            args.0.devid = id;

            if let Err(e) = btrfs_ioctl(self.fs.as_fd(), BTRFS_IOC_DEV_INFO, &mut args) {
                if let Some(libc::ENODEV) = e.raw_os_error() {
                    continue;
                }
                return Some(Err(e));
            }
            self.last_id = id;
            self.found_devices += 1;

            if self.found_devices > self.num_devices {
                return Some(Err(ErrorKind::InvalidData.into()));
            }
            break;
        }

        Some(Ok(args))
    }

    fn size_hint(&self) -> (usize, Option<usize>)
    {
        let hint = (self.num_devices - self.found_devices) as usize;

        (hint, Some(hint))
    }
}
