#![allow(missing_docs)]
//! Module to work with a btrfs send stream
//!
//! For documentation on BTRFS Send Stream, please see:
//! [btrfs.readthedocs.io - dev-send-stream](https://btrfs.readthedocs.io/en/latest/dev/dev-send-stream.html).
mod recieve;
mod send;

pub use recieve::receive_stream;
pub use send::SendBuilder;
