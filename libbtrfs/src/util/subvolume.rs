use crate::{bindings::btrfs_ioctl_get_subvol_info_args, tree_search::tree_item::RootItem};
use std::{
    ffi::{OsStr, c_char},
    fs::OpenOptions,
    io::{self, ErrorKind},
    os::fd::OwnedFd,
    os::unix::{ffi::OsStrExt, fs::OpenOptionsExt},
    path::{Component, MAIN_SEPARATOR, Path},
    ptr::write,
};

/// Fill the name field for btrfs_ioctl_vol_args_v2 or btrfs_ioctl_vol_args.
pub fn set_vol_name<const N: usize>(name: &[u8], dst: &mut [c_char; N]) -> io::Result<()>
{
    (name.contains(&0) || !copy_bytes_to_c_array!(name, dst))
        .then(|| ErrorKind::InvalidInput.into())
        .map_or(Ok(()), Err)
}

#[inline(always)]
fn parse_parent_and_name(path: &Path) -> (&OsStr, &[u8])
{
    let mut bytes = path.as_os_str().as_bytes();

    if let Some(p) = bytes.iter().rposition(|b| *b != MAIN_SEPARATOR as u8) {
        bytes = &bytes[..p + 1];
    }

    let (dirname, basename) = match bytes.iter().rposition(|b| *b == MAIN_SEPARATOR as u8) {
        Some(p) => (
            if p == 0 {
                Component::RootDir.as_os_str()
            } else {
                OsStr::from_bytes(&bytes[..p])
            },
            &bytes[p + 1..],
        ),
        None => (Component::CurDir.as_os_str(), bytes),
    };

    (dirname, basename)
}

/// Returns the parent dir and name as a tuple for a given path.
/// If the path contains invalid utf-8 then [`InvalidData`] is returned
pub fn open_parent_with_name(path: &Path) -> io::Result<(OwnedFd, &[u8])>
{
    let (dirname, basename) = parse_parent_and_name(path);

    let parent = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY)
        .open(dirname)?;

    Ok((parent.into(), basename))
}

/// Fill the relevant fields of a [`btrfs_ioctl_get_subvol_info_args`] struct from a [`RootItem`].
pub fn subvol_info_args_from_root_item(
    ri: RootItem<'_>,
    args: *mut btrfs_ioctl_get_subvol_info_args,
)
{
    unsafe {
        write(&raw mut (*args).generation, ri.generation());
        write(&raw mut (*args).flags, ri.flags());
        write(&raw mut (*args).uuid, ri.uuid().into_bytes());
        write(&raw mut (*args).parent_uuid, ri.parent_uuid().into_bytes());
        write(
            &raw mut (*args).received_uuid,
            ri.received_uuid().into_bytes(),
        );
        write(&raw mut (*args).ctransid, ri.ctransid());
        write(&raw mut (*args).otransid, ri.otransid());
        write(&raw mut (*args).stransid, ri.stransid());
        write(&raw mut (*args).rtransid, ri.rtransid());
        write(&raw mut (*args).ctime, ri.ctime().into());
        write(&raw mut (*args).otime, ri.otime().into());
        write(&raw mut (*args).stime, ri.stime().into());
        write(&raw mut (*args).rtime, ri.rtime().into());
    }
}

#[cfg(test)]
mod tests
{
    use super::*;
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;

    #[test]
    fn test_parsing_parent_and_name()
    {
        let mut path;
        let mut dir;
        let mut name;
        // ================================================================
        path = Path::new("/foo");
        (dir, name) = parse_parent_and_name(path);

        assert_eq!("/", dir);
        assert_eq!(OsStr::from_bytes(b"foo"), OsStr::from_bytes(name));
        // ================================================================
        path = Path::new("/abc///");
        (dir, name) = parse_parent_and_name(path);

        assert_eq!("/", dir);
        assert_eq!(OsStr::from_bytes(b"abc"), OsStr::from_bytes(name));
        // ================================================================
        path = Path::new("/");
        (dir, name) = parse_parent_and_name(path);

        assert_eq!("/", dir);
        assert_eq!(OsStr::from_bytes(b""), OsStr::from_bytes(name));
        // ================================================================
        path = Path::new("/a/b/c///");
        (dir, name) = parse_parent_and_name(path);

        assert_eq!("/a/b", dir);
        assert_eq!(OsStr::from_bytes(b"c"), OsStr::from_bytes(name));
        // ================================================================
        path = Path::new("a/b/c///");
        (dir, name) = parse_parent_and_name(path);

        assert_eq!("a/b", dir);
        assert_eq!(OsStr::from_bytes(b"c"), OsStr::from_bytes(name));
        // ================================================================
        path = Path::new("a/b/c///a/");
        (dir, name) = parse_parent_and_name(path);

        assert_eq!("a/b/c//", dir);
        assert_eq!(OsStr::from_bytes(b"a"), OsStr::from_bytes(name));
    }
}
