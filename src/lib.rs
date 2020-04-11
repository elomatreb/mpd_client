#![warn(
    missing_copy_implementations,
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub,
    unused_import_braces,
    unused_qualifications
)]
#![forbid(unsafe_code)]

//! User-friendly async client for [MPD](https://musicpd.org).

pub mod client;
pub mod errors;
pub mod filter;
pub mod util;

pub use client::Client;
pub use filter::Filter;
pub use util::Subsystem;

pub use mpd_protocol::{
    command_list,
    response::{Error, Frame},
    Command, CommandList, MpdCodecError,
};
