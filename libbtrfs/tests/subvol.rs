//! Subvol tests
//!
//! NOTE:
//! Subvol destroy ioctl cannot be used without elevated capability's, unless the filesystem is
//! mounted with `user_subvol_rm_allowed`. However, since Since 4.18 the rmdir_subvol feature allow
//! removing a subvolume with the rmdir(2) system call. This means the testing subvolumes will get
//! removed when the TempDir goes out of scope.
//! SEE: btrfs(5) - FILESYSTEM FEATURES, rmdir_subvol
//! <https://manpages.debian.org/bullseye/btrfs-progs/btrfs.5.en.html#FILESYSTEM_FEATURES>
use std::{env, io::ErrorKind, path::PathBuf};
use tempfile::TempDir;

use libbtrfs::subvol;

struct Sandbox
{
    pub _dir: TempDir,
    pub path: PathBuf,
}

impl Sandbox
{
    fn new() -> std::io::Result<Self>
    {
        let base = env::var("SANDBOX_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| env::temp_dir());

        let tmp_dir = tempfile::Builder::new()
            .prefix("__btrfs_test_sandbox__")
            .tempdir_in(base)?;

        if !libbtrfs::fs::is_btrfs(tmp_dir.path())? {
            return Err(ErrorKind::Unsupported.into());
        }

        let path = tmp_dir.path().to_path_buf();

        Ok(Self { path, _dir: tmp_dir })
    }
}

#[test]
fn create_subvol()
{
    let sandbox = Sandbox::new().expect("couldn't set up testing enviroment");

    let subvol = sandbox.path.join("_name");

    subvol::create(&subvol).expect("failed to create subvolume");

    let valid_subvol = libbtrfs::subvol::is_subvol(&subvol).expect("is subvol failed");

    assert!(valid_subvol)
}
