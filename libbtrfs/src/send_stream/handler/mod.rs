//! Module for handling send commands.
//!
//! # Example
//!
//! This example creates a send stream handler that will calculate the total bytes used by a
//! subvolume. This example illustrates how handlers can contain shared data. Note that a more
//! robust handler may choose to override the `StreamHandler::handle_cmd()` method to avoid parsing
//! data for ignored commands. This example omits many real world edge cases for the sake of
//! simplicity.
//!
//! ```no_run,rustfmt_skip
//! use libbtrfs::send_stream::{self, handler::{StreamHandler, command::*}};
//! use std::sync::{Arc, atomic::{AtomicU64, Ordering}};
//!
//! // approximate total disk usage for `subvol`.
//! fn subvolume_disk_usage(subvol: &str) -> std::io::Result<u64>
//! {
//!     let total_bytes: Arc<AtomicU64> = Default::default();
//!
//!     send_stream::SendBuilder::from_path(subvol)?
//!         .handler(HandleDiskUsage::new(total_bytes.clone()))
//!         .blocking_send()
//!         .map(|_| total_bytes.load(Ordering::Acquire))
//! }
//!
//! // data that is being shared with that handler should be wrapped in a Arc
//! // Shared data can be updated with the Drop trait to limit the amount of atomic operations
//! struct HandleDiskUsage
//! {
//!     total_bytes: u64,
//!     shared: Arc<AtomicU64>,
//! }
//!
//! impl HandleDiskUsage
//! {
//!     fn new(shared: Arc<AtomicU64>) -> Self
//!     {
//!         Self { total_bytes: shared.load(Ordering::Relaxed), shared }
//!     }
//! }
//!
//! // atomic data updated on Drop
//! impl Drop for HandleDiskUsage
//! {
//!     fn drop(&mut self)
//!     {
//!         self.shared.store(self.total_bytes, Ordering::Release)
//!     }
//! }
//!
//! impl StreamHandler for HandleDiskUsage
//! {
//!     // increment total bytes on write commands
//!     fn write(&mut self, WriteCmd { data, .. }: WriteCmd) -> std::io::Result<Option<()>>
//!     {
//!         self.total_bytes += data.len() as u64;
//!
//!         Ok(Some(()))
//!     }
//!
//!     // return None on the end command to end the receive
//!     fn end(&mut self, _: EndCmd) -> std::io::Result<Option<()>>
//!     {
//!         Ok(None)
//!     }
//!
//!     // ignore all other commands
//!     fn subvol(&mut self, _: SubvolCmd) -> std::io::Result<Option<()>> { Ok(Some(())) }
//!     fn snapshot(&mut self, _: SnapshotCmd) -> std::io::Result<Option<()>> { Ok(Some(())) }
//!     fn mkfile(&mut self, _: MkfileCmd) -> std::io::Result<Option<()>> { Ok(Some(())) }
//!     fn mkdir(&mut self, _: MkdirCmd) -> std::io::Result<Option<()>> { Ok(Some(())) }
//!     fn mknod(&mut self, _: MknodCmd) -> std::io::Result<Option<()>> { Ok(Some(())) }
//!     fn mkfifo(&mut self, _: MkfifoCmd) -> std::io::Result<Option<()>> { Ok(Some(())) }
//!     fn mksock(&mut self, _: MksockCmd) -> std::io::Result<Option<()>> { Ok(Some(())) }
//!     fn symlink(&mut self, _: SymlinkCmd) -> std::io::Result<Option<()>> { Ok(Some(())) }
//!     fn rename(&mut self, _: RenameCmd) -> std::io::Result<Option<()>> { Ok(Some(())) }
//!     fn link(&mut self, _: LinkCmd) -> std::io::Result<Option<()>> { Ok(Some(())) }
//!     fn unlink(&mut self, _: UnlinkCmd) -> std::io::Result<Option<()>> { Ok(Some(())) }
//!     fn rmdir(&mut self, _: RmdirCmd) -> std::io::Result<Option<()>> { Ok(Some(())) }
//!     fn set_xattr(&mut self, _: SetXattrCmd) -> std::io::Result<Option<()>> { Ok(Some(())) }
//!     fn remove_xattr(&mut self, _: RemoveXattrCmd) -> std::io::Result<Option<()>> { Ok(Some(())) }
//!     fn clone(&mut self, _: CloneCmd) -> std::io::Result<Option<()>> { Ok(Some(())) }
//!     fn truncate(&mut self, _: TruncateCmd) -> std::io::Result<Option<()>> { Ok(Some(())) }
//!     fn chmod(&mut self, _: ChmodCmd) -> std::io::Result<Option<()>> { Ok(Some(())) }
//!     fn chown(&mut self, _: ChownCmd) -> std::io::Result<Option<()>> { Ok(Some(())) }
//!     fn utimes(&mut self, _: UtimesCmd) -> std::io::Result<Option<()>> { Ok(Some(())) }
//!     fn update_extent(&mut self, _: UpdateExtentCmd) -> std::io::Result<Option<()>> { Ok(Some(())) }
//!     fn fallocate(&mut self, _: FallocateCmd) -> std::io::Result<Option<()>> { Ok(Some(())) }
//!     fn fileattr(&mut self, _: FileattrCmd) -> std::io::Result<Option<()>> { Ok(Some(())) }
//!     fn encoded_write(&mut self, _: EncodedWriteCmd) -> std::io::Result<Option<()>> { Ok(Some(())) }
//! }
//! ```

mod handle_full;

/// Send commands passed to the handler.
pub mod command;

use command::*;

pub use handle_full::HandleFull;

/// Handler trait for a BTRFS send stream.
///
/// Implementers of this trait can either be passed to the [`super::receive_stream()`] function
/// to handle a send stream directly, or be passed to the [`super::SendBuilder`], via the
/// [`super::SendBuilder::handler()`] method.
///
/// All trait methods except [`StreamHandler::handle_cmd()`] return an
/// [`std::io::Result<Option<()>>`]. The handler will continue handling command until a trait
/// method returns `Ok(None)`. In most cases this will be the [`StreamHandler::end()`] method.
pub trait StreamHandler: Send
{
    /// Start of commands for a subvolume.
    fn subvol(&mut self, _: SubvolCmd) -> std::io::Result<Option<()>>;
    /// Start of commands for a snapshot.
    fn snapshot(&mut self, _: SnapshotCmd) -> std::io::Result<Option<()>>;
    /// Command to create a regular file.
    fn mkfile(&mut self, _: MkfileCmd) -> std::io::Result<Option<()>>;
    /// Command to create a directory.
    fn mkdir(&mut self, _: MkdirCmd) -> std::io::Result<Option<()>>;
    /// Command to create sepcial device node.
    fn mknod(&mut self, _: MknodCmd) -> std::io::Result<Option<()>>;
    /// Command to create fifo.
    fn mkfifo(&mut self, _: MkfifoCmd) -> std::io::Result<Option<()>>;
    /// Command to create socket.
    fn mksock(&mut self, _: MksockCmd) -> std::io::Result<Option<()>>;
    /// Command to create symlink.
    fn symlink(&mut self, _: SymlinkCmd) -> std::io::Result<Option<()>>;
    /// Command to rename a file.
    fn rename(&mut self, _: RenameCmd) -> std::io::Result<Option<()>>;
    /// Command to rename a file.
    fn link(&mut self, _: LinkCmd) -> std::io::Result<Option<()>>;
    /// Command to create a hardlink.
    fn unlink(&mut self, _: UnlinkCmd) -> std::io::Result<Option<()>>;
    /// Command to remove a directory.
    fn rmdir(&mut self, _: RmdirCmd) -> std::io::Result<Option<()>>;
    /// Command to remove a extended attributes.
    fn set_xattr(&mut self, _: SetXattrCmd) -> std::io::Result<Option<()>>;
    /// Command remove extended attributes.
    fn remove_xattr(&mut self, _: RemoveXattrCmd) -> std::io::Result<Option<()>>;
    /// Write command.
    fn write(&mut self, _: WriteCmd) -> std::io::Result<Option<()>>;
    /// Clone extents.
    fn clone(&mut self, _: CloneCmd) -> std::io::Result<Option<()>>;
    /// Truncate a file.
    fn truncate(&mut self, _: TruncateCmd) -> std::io::Result<Option<()>>;
    /// Change file mode.
    fn chmod(&mut self, _: ChmodCmd) -> std::io::Result<Option<()>>;
    /// Change file owner (uid) and group (gid).
    fn chown(&mut self, _: ChownCmd) -> std::io::Result<Option<()>>;
    /// Change file timestamp
    fn utimes(&mut self, _: UtimesCmd) -> std::io::Result<Option<()>>;
    /// End of current subvolume or snapshot.
    fn end(&mut self, _: EndCmd) -> std::io::Result<Option<()>>;
    /// Encounterd for a stream sent without file data, [`super::SendBuilder::no_file_data()`], set
    /// to true. Contains information about extent changes but does not contain file data.
    fn update_extent(&mut self, _: UpdateExtentCmd) -> std::io::Result<Option<()>>;

    // ==========================================================================
    // VERSION 2
    // ==========================================================================

    /// Change file extents.
    fn fallocate(&mut self, _: FallocateCmd) -> std::io::Result<Option<()>>;
    /// File attributes.
    fn fileattr(&mut self, _: FileattrCmd) -> std::io::Result<Option<()>>;
    /// Write compressed data directly to the filesystem.
    fn encoded_write(&mut self, _: EncodedWriteCmd) -> std::io::Result<Option<()>>;

    /// Handles each command.
    fn handle_cmd(&mut self, cmd: u16, data: &[u8], version: u32) -> std::io::Result<Option<()>>
    {
        match cmd {
            SubvolCmd::KEY => self.subvol(SendCmd::parse_tlv(data)?),
            SnapshotCmd::KEY => self.snapshot(SendCmd::parse_tlv(data)?),
            MkfileCmd::KEY => self.mkfile(SendCmd::parse_tlv(data)?),
            MkdirCmd::KEY => self.mkdir(SendCmd::parse_tlv(data)?),
            MknodCmd::KEY => self.mknod(SendCmd::parse_tlv(data)?),
            MkfifoCmd::KEY => self.mkfifo(SendCmd::parse_tlv(data)?),
            MksockCmd::KEY => self.mksock(SendCmd::parse_tlv(data)?),
            SymlinkCmd::KEY => self.symlink(SendCmd::parse_tlv(data)?),
            RenameCmd::KEY => self.rename(SendCmd::parse_tlv(data)?),
            LinkCmd::KEY => self.link(SendCmd::parse_tlv(data)?),
            UnlinkCmd::KEY => self.unlink(SendCmd::parse_tlv(data)?),
            RmdirCmd::KEY => self.rmdir(SendCmd::parse_tlv(data)?),
            SetXattrCmd::KEY => self.set_xattr(SendCmd::parse_tlv(data)?),
            RemoveXattrCmd::KEY => self.remove_xattr(SendCmd::parse_tlv(data)?),
            WriteCmd::KEY => self.write(if version == 2 {
                SendDataCmd::parse_tlv_v2(data)?
            } else {
                SendCmd::parse_tlv(data)?
            }),
            CloneCmd::KEY => self.clone(SendCmd::parse_tlv(data)?),
            TruncateCmd::KEY => self.truncate(SendCmd::parse_tlv(data)?),
            ChmodCmd::KEY => self.chmod(SendCmd::parse_tlv(data)?),
            ChownCmd::KEY => self.chown(SendCmd::parse_tlv(data)?),
            UtimesCmd::KEY => self.utimes(SendCmd::parse_tlv(data)?),
            EndCmd::KEY => self.end(SendCmd::parse_tlv(data)?),
            UpdateExtentCmd::KEY => self.update_extent(SendCmd::parse_tlv(data)?),
            // ==========================================================================
            // VERSION 2
            // ==========================================================================
            FallocateCmd::KEY => self.fallocate(SendCmd::parse_tlv(data)?),
            FileattrCmd::KEY => self.fileattr(SendCmd::parse_tlv(data)?),
            EncodedWriteCmd::KEY => self.encoded_write(if version == 2 {
                SendDataCmd::parse_tlv_v2(data)?
            } else {
                SendCmd::parse_tlv(data)?
            }),
            _ => receive_error!("UNEXPECTED COMMAND"),
        }
    }
}
