//! Btrfs device operations.
//!
//! Basic management of devices in a btrfs filesystem such as adding and removing devices as
//! well as structures which provide information about devices in a btrfs filesystem.
use crate::{
    bindings::{
        BTRFS_DEV_STAT_CORRUPTION_ERRS, BTRFS_DEV_STAT_FLUSH_ERRS, BTRFS_DEV_STAT_GENERATION_ERRS,
        BTRFS_DEV_STAT_READ_ERRS, BTRFS_DEV_STAT_VALUES_MAX, BTRFS_DEV_STAT_WRITE_ERRS,
        BTRFS_DEVICE_SPEC_BY_ID, BTRFS_IOC_ADD_DEV, BTRFS_IOC_DEV_INFO, BTRFS_IOC_GET_DEV_STATS,
        BTRFS_IOC_RM_DEV, BTRFS_IOC_RM_DEV_V2, btrfs_ioctl_dev_info_args,
        btrfs_ioctl_get_dev_stats, btrfs_ioctl_vol_args, btrfs_ioctl_vol_args_v2,
    },
    util::{IoResult, btrfs_ioctl, set_vol_name},
};
use std::{
    fs::File,
    io::ErrorKind,
    mem::MaybeUninit,
    os::{fd::AsFd, unix::ffi::OsStrExt},
    path::Path,
};

mod info;
pub use info::{DevInfo, boxed_info, info, iter};

/// Stats for a btrfs device
///
/// Returned by the [`get_stats()`] function
#[repr(transparent)]
pub struct DevStats(btrfs_ioctl_get_dev_stats);

impl DevStats
{
    /// Number of write errors for this device
    pub fn write_errors(&self) -> u64
    {
        self.0.values[BTRFS_DEV_STAT_WRITE_ERRS as usize]
    }

    /// Number of read errors for this device
    pub fn read_errors(&self) -> u64
    {
        self.0.values[BTRFS_DEV_STAT_READ_ERRS as usize]
    }

    /// Number of flush errors for this device
    pub fn flush_errors(&self) -> u64
    {
        self.0.values[BTRFS_DEV_STAT_FLUSH_ERRS as usize]
    }

    /// Number of corruptions errors for this device
    pub fn corruptions_errors(&self) -> u64
    {
        self.0.values[BTRFS_DEV_STAT_CORRUPTION_ERRS as usize]
    }

    /// Number of generation errors for this device
    pub fn generation_errors(&self) -> u64
    {
        self.0.values[BTRFS_DEV_STAT_GENERATION_ERRS as usize]
    }

    /// Returns a boolean indicating if this device has any errors
    pub fn has_errors(&self) -> bool
    {
        self.0.values.iter().any(|&f| f != 0)
    }
}

/// Returns a [`DevStats`] struct.
pub fn get_stats<P: AsRef<Path>>(devid: u64, fs: P) -> IoResult<DevStats>
{
    File::open(fs).and_then(|f| io::get_stats(devid, f))
}

/// Returns a [`DevStats`] struct that has been allocated on the heap.
pub fn get_boxed_stats<P: AsRef<Path>>(devid: u64, fs: P) -> IoResult<Box<DevStats>>
{
    File::open(fs).and_then(|f| io::get_boxed_stats(devid, f))
}

/// Adds a device to a btrfs filesystem
///
/// # Notes
///
/// **Requires CAP_SYS_ADMIN capabilities**
pub fn add<P: AsRef<Path>>(device: P, fs: P) -> IoResult<()>
{
    File::open(fs).and_then(|f| io::add(device, f))
}

/// Removes a device from a btrfs filesystem by device name
///
/// # Notes
///
/// **Requires CAP_SYS_ADMIN capabilities**
pub fn rm<P: AsRef<Path>>(device: P, fs: P) -> IoResult<()>
{
    File::open(fs).and_then(|f| io::rm(device, f))
}

/// Remove a btrfs device from the filesystem by device id
///
/// # Notes
///
/// **Requires CAP_SYS_ADMIN capabilities**
pub fn rm_by_id<P: AsRef<Path>>(devid: u64, fs: P) -> IoResult<()>
{
    File::open(fs).and_then(|f| io::rm_by_id(devid, f))
}

/// Entry for I/O resources.
pub mod io
{
    use super::*;

    pub use info::io::{boxed_info, info, iter};

    /// See [super::get_stats()]
    pub fn get_stats<R: AsFd>(devid: u64, resource: R) -> IoResult<DevStats>
    {
        let mut stats_args: DevStats = unsafe { MaybeUninit::zeroed().assume_init() };

        stats_args.0.devid = devid;
        stats_args.0.nr_items = BTRFS_DEV_STAT_VALUES_MAX as u64;

        btrfs_ioctl(resource.as_fd(), BTRFS_IOC_GET_DEV_STATS, &mut stats_args).map(|_| stats_args)
    }

    /// See [super::get_boxed_stats()]
    pub fn get_boxed_stats<R: AsFd>(devid: u64, resource: R) -> IoResult<Box<DevStats>>
    {
        let mut stats_args: Box<DevStats> = unsafe { Box::new_zeroed().assume_init() };

        stats_args.0.devid = devid;
        stats_args.0.nr_items = BTRFS_DEV_STAT_VALUES_MAX as u64;

        btrfs_ioctl(
            resource.as_fd(),
            BTRFS_IOC_GET_DEV_STATS,
            stats_args.as_mut(),
        )
        .map(|_| stats_args)
    }

    /// See [super::add()]
    pub fn add<R: AsFd, P: AsRef<Path>>(device: P, resource: R) -> IoResult<()>
    {
        let mut vol_args: btrfs_ioctl_vol_args = unsafe { MaybeUninit::zeroed().assume_init() };

        set_vol_name(device.as_ref().as_os_str().as_bytes(), &mut vol_args.name)
            .and_then(|_| btrfs_ioctl(resource.as_fd(), BTRFS_IOC_ADD_DEV, &mut vol_args))
    }

    /// See [super::rm()]
    pub fn rm<R: AsFd, P: AsRef<Path>>(device: P, resource: R) -> IoResult<()>
    {
        fn rm_dev_v1<R: AsFd>(device: &[u8], resource: R) -> IoResult<()>
        {
            let mut vol_args: btrfs_ioctl_vol_args = unsafe { MaybeUninit::zeroed().assume_init() };

            set_vol_name(device, &mut vol_args.name)
                .and_then(|_| btrfs_ioctl(resource, BTRFS_IOC_RM_DEV, &mut vol_args))
        }
        let mut vol_args: btrfs_ioctl_vol_args_v2 = unsafe { MaybeUninit::zeroed().assume_init() };
        let device = device.as_ref().as_os_str().as_bytes();

        let res = set_vol_name(device, unsafe { &mut vol_args.inner2.name })
            .and_then(|_| btrfs_ioctl(resource.as_fd(), BTRFS_IOC_RM_DEV_V2, &mut vol_args));

        if res.as_ref().is_err_and(|e| {
            e.raw_os_error() == Some(libc::ENOTTY) || e.raw_os_error() == Some(libc::EOPNOTSUPP)
        }) {
            return rm_dev_v1(device, resource);
        }

        res
    }

    /// See [super::rm_by_id()]
    pub fn rm_by_id<R: AsFd>(devid: u64, resource: R) -> IoResult<()>
    {
        let mut vol_args: btrfs_ioctl_vol_args_v2 = unsafe { MaybeUninit::zeroed().assume_init() };

        vol_args.flags = BTRFS_DEVICE_SPEC_BY_ID;
        vol_args.inner2.devid = devid;

        btrfs_ioctl(resource.as_fd(), BTRFS_IOC_RM_DEV_V2, &mut vol_args)
    }
}
