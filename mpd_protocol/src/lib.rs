#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]
#![deny(intra_doc_link_resolution_failure)]
#![forbid(unsafe_code)]

//! Implementation of the client protocol for [MPD]. Supports binary responses and command lists,
//! provided they are initiated with the `command_list_ok_begin` command.
//!
//! Consists of a parser for MPD responses ([`parser`]), and an implementation of [Tokio]'s
//! [codec][tokio-codec] subsystem to facilitate asynchronous clients ([`codec`]).
//!
//! [MPD]: https://musicpd.org
//! [Tokio]: https://tokio.rs
//! [tokio-codec]: https://docs.rs/tokio-util/0.3.0/tokio_util/codec/index.html

pub mod codec;
pub mod command;
pub mod parser;
pub mod response;

mod macros;

pub use codec::{MpdCodec, MpdCodecError};
pub use command::{Command, CommandList};
pub use parser::{greeting as parse_greeting, response as parse_response};
pub use response::Response;
