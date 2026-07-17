//! Btrfs filesystem block groups and raid profiles
#![allow(missing_docs)]
use crate::{
    bindings::{
        BTRFS_BLOCK_GROUP_DATA, BTRFS_BLOCK_GROUP_DUP, BTRFS_BLOCK_GROUP_METADATA,
        BTRFS_BLOCK_GROUP_PROFILE_MASK, BTRFS_BLOCK_GROUP_RAID0, BTRFS_BLOCK_GROUP_RAID1,
        BTRFS_BLOCK_GROUP_RAID1C3, BTRFS_BLOCK_GROUP_RAID1C4, BTRFS_BLOCK_GROUP_RAID5,
        BTRFS_BLOCK_GROUP_RAID6, BTRFS_BLOCK_GROUP_RAID10, BTRFS_BLOCK_GROUP_RESERVED,
        BTRFS_BLOCK_GROUP_SYSTEM, BTRFS_BLOCK_GROUP_TYPE_MASK, BTRFS_ERROR_DEV_RAID1_MIN_NOT_MET,
        BTRFS_ERROR_DEV_RAID1C3_MIN_NOT_MET, BTRFS_ERROR_DEV_RAID1C4_MIN_NOT_MET,
        BTRFS_ERROR_DEV_RAID5_MIN_NOT_MET, BTRFS_ERROR_DEV_RAID6_MIN_NOT_MET,
        BTRFS_ERROR_DEV_RAID10_MIN_NOT_MET, BTRFS_SPACE_INFO_GLOBAL_RSV,
    },
    util::IoError,
};
use std::io::ErrorKind;

/// Possible block group types for btrfs
///
/// More information on btrfs block groups can be found at [`btrfs.readthedocs.io`](
/// https://btrfs.readthedocs.io/en/latest/mkfs.btrfs.html#block-groups-chunks-raid).
///
/// `From<u64>` can be used to determine which block group the raw flags from
/// [`super::SpaceInfo::flags`] corresponds to
#[derive(Debug)]
pub enum BlockType
{
    Data,
    System,
    Metadata,
    GlobalReserve,
    Unknown,
}

/// Converts raw flags to its corresponding block type
impl From<u64> for BlockType
{
    fn from(value: u64) -> Self
    {
        match value & (BTRFS_BLOCK_GROUP_TYPE_MASK | BTRFS_SPACE_INFO_GLOBAL_RSV) {
            BTRFS_BLOCK_GROUP_DATA => Self::Data,
            BTRFS_BLOCK_GROUP_SYSTEM => Self::System,
            BTRFS_BLOCK_GROUP_METADATA => Self::Metadata,
            BTRFS_SPACE_INFO_GLOBAL_RSV => Self::GlobalReserve,
            _ => Self::Unknown,
        }
    }
}

/// Structure containing information concerning a btrfs block group profile
///
/// More information about btrfs RAID profiles can be found at [`btrfs.readthedocs.io`](
/// https://btrfs.readthedocs.io/en/latest/mkfs.btrfs.html#man-mkfs-profiles).
///
/// `From<u64>` will convert raw flags from [`super::SpaceInfo::flags`] to the corresponding
/// [`RaidProfile`]
#[derive(Clone, Copy, Debug)]
pub struct RaidProfile
{
    pub sub_stripes: u32,
    pub dev_stripes: u32,
    pub devs_max: u32,
    pub devs_min: u32,
    pub tolerated_failures: u32,
    pub devs_increment: u32,
    pub ncopies: u32,
    pub nparity: u32,
    pub mindev_error: u32,
    pub lower_name: &'static str,
    pub upper_name: &'static str,
    pub bg_flag: u64,
}

/// Converts raw flags to the corresponding raid profile
impl TryFrom<u64> for RaidProfile
{
    type Error = IoError;

    fn try_from(mut value: u64) -> Result<Self, Self::Error>
    {
        value &= !(BTRFS_BLOCK_GROUP_TYPE_MASK | BTRFS_BLOCK_GROUP_RESERVED);

        // Note: Using a match on `value` does seem to work but with limited test cases.
        // Using a if/else for now to be safe in case there are edge cases were a flag can have
        // more than one bit and ordering of the if statment is important
        let index = if value & !BTRFS_BLOCK_GROUP_PROFILE_MASK != 0 {
            return Err(ErrorKind::InvalidInput.into());
        } else if value & BTRFS_BLOCK_GROUP_RAID10 != 0 {
            RaidIndex::RAID10
        } else if value & BTRFS_BLOCK_GROUP_RAID1 != 0 {
            RaidIndex::RAID1
        } else if value & BTRFS_BLOCK_GROUP_RAID1C3 != 0 {
            RaidIndex::RAID1C3
        } else if value & BTRFS_BLOCK_GROUP_RAID1C4 != 0 {
            RaidIndex::RAID1C4
        } else if value & BTRFS_BLOCK_GROUP_DUP != 0 {
            RaidIndex::DUP
        } else if value & BTRFS_BLOCK_GROUP_RAID0 != 0 {
            RaidIndex::RAID0
        } else if value & BTRFS_BLOCK_GROUP_RAID5 != 0 {
            RaidIndex::RAID5
        } else if value & BTRFS_BLOCK_GROUP_RAID6 != 0 {
            RaidIndex::RAID6
        } else {
            RaidIndex::Single
        };

        Ok(RAID_ARRAY[index as usize])
    }
}

/// Used to index into the [RAID_ARRAY]
enum RaidIndex
{
    RAID10 = 0,
    RAID1,
    DUP,
    RAID0,
    Single,
    RAID5,
    RAID6,
    RAID1C3,
    RAID1C4,
    MAX,
}

/// Array containing all btrfs Raid Profiles
const RAID_ARRAY: [RaidProfile; RaidIndex::MAX as usize] = [
    BTRFS_RAID_RAID10,
    BTRFS_RAID_RAID1,
    BTRFS_RAID_DUP,
    BTRFS_RAID_RAID0,
    BTRFS_RAID_SINGLE,
    BTRFS_RAID_RAID5,
    BTRFS_RAID_RAID6,
    BTRFS_RAID_RAID1C3,
    BTRFS_RAID_RAID1C4,
];

const BTRFS_RAID_RAID10: RaidProfile = RaidProfile {
    sub_stripes: 2,
    dev_stripes: 1,
    devs_max: 0,
    devs_min: 2,
    tolerated_failures: 1,
    devs_increment: 2,
    ncopies: 2,
    nparity: 0,
    lower_name: "raid10",
    upper_name: "RAID10",
    bg_flag: BTRFS_BLOCK_GROUP_RAID10,
    mindev_error: BTRFS_ERROR_DEV_RAID10_MIN_NOT_MET,
};

const BTRFS_RAID_RAID1: RaidProfile = RaidProfile {
    sub_stripes: 1,
    dev_stripes: 1,
    devs_max: 2,
    devs_min: 2,
    tolerated_failures: 1,
    devs_increment: 2,
    ncopies: 2,
    nparity: 0,
    lower_name: "raid1",
    upper_name: "RAID1",
    bg_flag: BTRFS_BLOCK_GROUP_RAID1,
    mindev_error: BTRFS_ERROR_DEV_RAID1_MIN_NOT_MET,
};

const BTRFS_RAID_DUP: RaidProfile = RaidProfile {
    sub_stripes: 1,
    dev_stripes: 2,
    devs_max: 1,
    devs_min: 1,
    tolerated_failures: 0,
    devs_increment: 1,
    ncopies: 2,
    nparity: 0,
    lower_name: "dup",
    upper_name: "DUP",
    bg_flag: BTRFS_BLOCK_GROUP_DUP,
    mindev_error: 0,
};

const BTRFS_RAID_RAID0: RaidProfile = RaidProfile {
    sub_stripes: 1,
    dev_stripes: 1,
    devs_max: 0,
    devs_min: 1,
    tolerated_failures: 0,
    devs_increment: 1,
    ncopies: 1,
    nparity: 0,
    lower_name: "raid0",
    upper_name: "RAID0",
    bg_flag: BTRFS_BLOCK_GROUP_RAID0,
    mindev_error: 0,
};

const BTRFS_RAID_SINGLE: RaidProfile = RaidProfile {
    sub_stripes: 1,
    dev_stripes: 1,
    devs_max: 1,
    devs_min: 1,
    tolerated_failures: 0,
    devs_increment: 1,
    ncopies: 1,
    nparity: 0,
    lower_name: "single",
    upper_name: "SINGLE",
    bg_flag: 0,
    mindev_error: 0,
};

const BTRFS_RAID_RAID5: RaidProfile = RaidProfile {
    sub_stripes: 1,
    dev_stripes: 1,
    devs_max: 0,
    devs_min: 2,
    tolerated_failures: 1,
    devs_increment: 1,
    ncopies: 1,
    nparity: 1,
    lower_name: "raid5",
    upper_name: "RAID5",
    bg_flag: BTRFS_BLOCK_GROUP_RAID5,
    mindev_error: BTRFS_ERROR_DEV_RAID5_MIN_NOT_MET,
};

const BTRFS_RAID_RAID6: RaidProfile = RaidProfile {
    sub_stripes: 1,
    dev_stripes: 1,
    devs_max: 0,
    devs_min: 3,
    tolerated_failures: 2,
    devs_increment: 1,
    ncopies: 1,
    nparity: 2,
    lower_name: "raid6",
    upper_name: "RAID6",
    bg_flag: BTRFS_BLOCK_GROUP_RAID6,
    mindev_error: BTRFS_ERROR_DEV_RAID6_MIN_NOT_MET,
};

const BTRFS_RAID_RAID1C3: RaidProfile = RaidProfile {
    sub_stripes: 1,
    dev_stripes: 1,
    devs_max: 3,
    devs_min: 3,
    tolerated_failures: 2,
    devs_increment: 3,
    ncopies: 3,
    nparity: 0,
    lower_name: "raid1c3",
    upper_name: "RAID1C3",
    bg_flag: BTRFS_BLOCK_GROUP_RAID1C3,
    mindev_error: BTRFS_ERROR_DEV_RAID1C3_MIN_NOT_MET,
};

const BTRFS_RAID_RAID1C4: RaidProfile = RaidProfile {
    sub_stripes: 1,
    dev_stripes: 1,
    devs_max: 4,
    devs_min: 4,
    tolerated_failures: 3,
    devs_increment: 4,
    ncopies: 4,
    nparity: 0,
    lower_name: "raid1c4",
    upper_name: "RAID1C4",
    bg_flag: BTRFS_BLOCK_GROUP_RAID1C4,
    mindev_error: BTRFS_ERROR_DEV_RAID1C4_MIN_NOT_MET,
};
