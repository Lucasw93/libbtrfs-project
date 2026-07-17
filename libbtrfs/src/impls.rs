mod default
{
    use crate::bindings::*;

    impl Default for btrfs_ioctl_search_key
    {
        #[inline]
        fn default() -> Self
        {
            Self {
                nr_items: u32::MAX,
                tree_id: BTRFS_ROOT_TREE_OBJECTID,
                min_objectid: u64::MIN,
                max_objectid: u64::MAX,
                min_offset: u64::MIN,
                max_offset: u64::MAX,
                min_transid: u64::MIN,
                max_transid: u64::MAX,
                min_type: u32::MIN,
                max_type: u32::MAX,
                unused: 0,
                unused1: 0,
                unused2: 0,
                unused3: 0,
                unused4: 0,
            }
        }
    }
}

#[cfg(feature = "debug")]
mod debug
{
    use crate::*;
    use std::fmt::{self, Debug, Formatter};

    impl Debug for subvol::SubvolInfo
    {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result
        {
            f.debug_struct("SubvolInfo")
                .field("treeid", &self.treeid())
                .field("name", &self.name_str())
                .field("parent_id", &self.parent_id())
                .field("dirid", &self.dirid())
                .field("generation", &self.generation())
                .field("flags", &self.flags())
                .field("uuid", &self.uuid())
                .field("parend_uuid", &self.parent_uuid())
                .field("received_uuid", &self.received_uuid())
                .field("ctransid", &self.ctransid())
                .field("otransid", &self.otransid())
                .field("stransid", &self.stransid())
                .field("rtransid", &self.rtransid())
                .field(
                    "ctime",
                    &format_args!(
                        "{{ sec: {}, nsec: {} }}",
                        &self.ctime().sec(),
                        &self.ctime().nsec(),
                    ),
                )
                .field(
                    "otime",
                    &format_args!(
                        "{{ sec: {}, nsec: {} }}",
                        &self.otime().sec(),
                        &self.otime().nsec()
                    ),
                )
                .field(
                    "stime",
                    &format_args!(
                        "{{ sec: {}, nsec: {} }}",
                        &self.stime().sec(),
                        &self.stime().nsec()
                    ),
                )
                .field(
                    "rtime",
                    &format_args!(
                        "{{ sec: {}, nsec: {} }}",
                        &self.rtime().sec(),
                        &self.rtime().nsec()
                    ),
                )
                .finish()
        }
    }

    impl Debug for fs::FsInfo
    {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result
        {
            f.debug_struct("FsInfo")
                .field("max_id", &self.max_id())
                .field("num_devices", &self.num_devices())
                .field("fsid", &self.fsid())
                .field("nodesize", &self.nodesize())
                .field("sectorsize", &self.sectorsize())
                .field("clone_alignment", &self.clone_alignment())
                .finish()
        }
    }

    impl Debug for fs::SpaceInfo
    {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result
        {
            f.debug_struct("SpaceInfo")
                .field("block_type", &self.block_type())
                .field("raid_profile", &self.raid_profile().map(|v| v.upper_name))
                .field("used_bytes", &self.used_bytes())
                .field("total_bytes", &self.total_bytes())
                .finish()
        }
    }

    impl Debug for crate::tree_search::SearchItem<'_>
    {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result
        {
            f.debug_struct("SearchItem")
                .field("objectid", &self.objectid())
                .field("type", &self.ty())
                .field("offset", &self.offset())
                .field("transid", &self.transid())
                .field("len", &self.len())
                .finish()
        }
    }
}
