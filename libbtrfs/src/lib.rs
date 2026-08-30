//! This crate is a rust library for working with the btrfs filesystem.
#![cfg(target_os = "linux")]

#[macro_use]
mod macros;
mod bindings;
mod flag_options;
mod impls;
mod util;

#[cfg(feature = "send-stream")]
pub mod send_stream;

pub mod dev;
pub mod ffi;
pub mod fs;
pub mod lookup;
pub mod subvol;
pub mod tree_search;

pub use flag_options::Flags;
pub use subvol::snap;
