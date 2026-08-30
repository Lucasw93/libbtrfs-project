//! Module to work with a btrfs send stream
//!
//! For additional documentation on BTRFS Send Stream, please see:
//! **[btrfs.readthedocs.io - dev-send-stream](https://btrfs.readthedocs.io/en/latest/dev/dev-send-stream.html).**
//!
//! # Example
//!
//! The following example creates a blocking send stream from `source`, where send command will be
//! handled by `handler`.
//!
//! ```no_run
//! use libbtrfs::send_stream;
//! use std::fs::File;
//!
//! // read-only source subvolume
//! let source = File::open("/path/to/read-only/subvolume")?;
//!
//! // mountpoint where the subvolume will be received
//! let mount = File::open("/receive/destination")?;
//!
//! // handler for send commands
//! let handler = send_stream::handler::HandleFull::new(mount)?;
//!
//! send_stream::SendBuilder::new(source)
//!     .handler(handler)
//!     .blocking_send()?;
//!
//! # Ok::<(), std::io::Error>(())
//! ```

// todo. better errors.
macro_rules! receive_error {
    ( $msg:literal ) => {
        Err(::std::io::Error::new(
            ::std::io::ErrorKind::Other,
            format!("[RECEIVE-ERROR]: {}", $msg),
        ))
    };
}

mod read_stream;
mod recieve;
mod send;

pub mod handler;

pub use recieve::receive_stream;
pub use send::{SendBuilder, SendHandle};
