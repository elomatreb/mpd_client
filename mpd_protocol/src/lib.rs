#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]
#![deny(broken_intra_doc_links)]
#![forbid(unsafe_code)]

//! Implementation of the client protocol for [MPD]. Supports binary responses and command lists.
//!
//! Primarily consists of an implementation of [Tokio]'s [codec][tokio-codec] subsystem.
//!
//! # Parser Support
//!
//! The response parser will understand command lists properly **only** if they are initiated with
//! the `command_list_ok_begin` command. If the command list is initiated without response
//! separators, all responses will be treated as a single large response which may result in
//! incorrect behavior.
//!
//! [MPD]: https://musicpd.org
//! [Tokio]: https://tokio.rs
//! [tokio-codec]: https://docs.rs/tokio-util/0.3.0/tokio_util/codec/index.html

pub mod codec;
pub mod command;
pub mod response;
pub mod sync;

mod parser;

pub use codec::{MpdCodec, MpdCodecError};
pub use command::{Command, CommandList};
pub use response::Response;
