#![cfg(target_os = "linux")]
//! This crate is a rust library for working with the btrfs filesystem.

#[cfg(target_pointer_width = "64")]
#[path = "generated_64.rs"]
#[rustfmt::skip]
mod bindings;

#[cfg(target_pointer_width = "32")]
#[path = "generated_32.rs"]
#[rustfmt::skip]
mod bindings;

#[macro_use]
mod macros;
mod flag_options;
mod impls;
mod util;

#[cfg(feature = "unstable")]
pub mod send_stream;

pub mod dev;
pub mod fs;
pub mod lookup;
pub mod subvol;
pub mod tree_search;

pub use flag_options::Flags;
pub use subvol::snap;
pub use util::KernelStr;
