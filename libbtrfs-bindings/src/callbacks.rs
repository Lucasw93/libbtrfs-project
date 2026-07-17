use bindgen::callbacks::{self, IntKind};

#[derive(Debug)]
pub struct Callbacks;

impl Callbacks
{
    const HELPER_PREFIX: &str = "RUST_CONST_HELPER";
}

impl callbacks::ParseCallbacks for Callbacks
{
    fn generated_name_override(&self, item: callbacks::ItemInfo<'_>) -> Option<String>
    {
        item.name
            .starts_with(Self::HELPER_PREFIX)
            .then(|| item.name[Self::HELPER_PREFIX.len() + 1..].into())
    }

    fn item_name(&self, item: callbacks::ItemInfo) -> Option<String>
    {
        match item.name {
            name if name.starts_with("btrfs_ioctl_vol_args_v2") => match name {
                "btrfs_ioctl_vol_args_v2__bindgen_ty_2" => Some("vol_args_v2_volume"),
                "btrfs_ioctl_vol_args_v2__bindgen_ty_1" => Some("vol_args_v2_qgroup"),
                "btrfs_ioctl_vol_args_v2__bindgen_ty_1__bindgen_ty_1" => {
                    Some("vol_args_v2_qgroup_opts")
                }
                _ => None,
            },
            _ => None,
        }
        .map(Into::into)
    }

    fn int_macro(&self, name: &str, _value: i64) -> Option<IntKind>
    {
        match name {
            "BTRFS_DEVICE_PATH_NAME_MAX"
            | "BTRFS_FSID_SIZE"
            | "BTRFS_LABEL_SIZE"
            | "BTRFS_PATH_NAME_MAX"
            | "BTRFS_UUID_SIZE"
            | "BTRFS_UUID_UNPARSED_SIZE"
            | "BTRFS_VOL_NAME_MAX" => Some(IntKind::Custom { name: "usize", is_signed: false }),

            "BTRFS_CSUM_SIZE"
            | "BTRFS_IOCTL_MAGIC"
            | "BTRFS_MAX_METADATA_BLOCKSIZE"
            | "BTRFS_SAME_DATA_DIFFERS" => None,

            _ if name.ends_with("KEY")
                || name.starts_with("BTRFS_SUBVOL_SYNC")
                || name.starts_with("BTRFS_ENCODED_IO") =>
            {
                None
            }
            _ if name.starts_with("BTRFS_FT") => Some(IntKind::U8),
            _ => Some(IntKind::U64),
        }
    }
}
