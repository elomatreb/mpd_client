#![warn(
    intra_doc_link_resolution_failure,
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub,
    unused_import_braces,
    unused_qualifications
)]
#![forbid(unsafe_code)]

//! Asynchronous client for [MPD](https://musicpd.org).
//!
//! The [`Client`] type is the primary API.

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

/// Protocol-level types.
pub mod raw {
    pub use mpd_protocol::{
        response::{Error as ErrorResponse, Frame},
        Command as RawCommand, CommandList as RawCommandList, MpdCodecError as ProtocolError,
    };
}

mod sealed {
    pub trait Sealed {}
}
