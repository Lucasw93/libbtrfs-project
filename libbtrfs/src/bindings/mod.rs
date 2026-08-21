#[cfg_attr(target_pointer_width = "32", path = "generated_32.rs")]
#[cfg_attr(target_pointer_width = "64", path = "generated_64.rs")]
#[rustfmt::skip]
mod generated;

pub use generated::*;
