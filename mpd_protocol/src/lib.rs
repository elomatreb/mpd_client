#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]
#![deny(broken_intra_doc_links)]
#![forbid(unsafe_code)]
#![cfg_attr(docsrs, feature(doc_cfg))]

//! Implementation of the client protocol for [MPD]. Supports binary responses and command lists.
//!
//! The async support, available if the `async` crate feature is enabled, consists of an
//! implementation of [Tokio]'s [codec][tokio-codec] subsystem.
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
//! [tokio-codec]: https://docs.rs/tokio-util/0.6.0/tokio_util/codec/index.html

use std::error::Error;
use std::fmt;
use std::io;

#[cfg(feature = "async")]
#[cfg_attr(docsrs, doc(cfg(feature = "async")))]
mod codec;

pub mod command;
pub mod response;
pub mod sync;

mod parser;

#[cfg(feature = "async")]
pub use codec::MpdCodec;

pub use command::{Command, CommandList};
pub use response::Response;

/// Unrecoverable errors.
#[derive(Debug)]
pub enum MpdProtocolError {
    /// IO error occured
    Io(io::Error),
    /// A message could not be parsed succesfully.
    InvalidMessage,
}

impl fmt::Display for MpdProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MpdProtocolError::Io(_) => write!(f, "IO error"),
            MpdProtocolError::InvalidMessage => write!(f, "invalid message"),
        }
    }
}

#[doc(hidden)]
impl From<io::Error> for MpdProtocolError {
    fn from(e: io::Error) -> Self {
        MpdProtocolError::Io(e)
    }
}

impl Error for MpdProtocolError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            MpdProtocolError::Io(e) => Some(e),
            _ => None,
        }
    }
}
