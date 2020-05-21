#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub,
    unused_import_braces,
    unused_qualifications
)]
#![forbid(unsafe_code)]

//! User-friendly async client for [MPD](https://musicpd.org).

mod client;
mod errors;

pub mod commands;
pub mod filter;
pub mod state_changes;
pub mod tag;

pub use client::{Client, ConnectResult};
pub use errors::CommandError;
pub use filter::Filter;
pub use state_changes::Subsystem;
pub use tag::Tag;

pub use mpd_protocol::{
    command_list,
    response::{Error as ErrorResponse, Frame},
    Command as RawCommand, CommandList, MpdCodecError,
};

mod sealed {
    pub trait Sealed {}
}
