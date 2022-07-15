#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]
#![deny(rustdoc::broken_intra_doc_links)]
#![forbid(unsafe_code)]
#![cfg_attr(docsrs, feature(doc_cfg))]

//! Implementation of the client protocol for [MPD]. Supports binary responses and command lists.
//!
//! # Crate Features
//!
//! | Feature | Description                     |
//! |---------|---------------------------------|
//! | `async` | Async support, based on [Tokio] |
//!
//! [MPD]: https://musicpd.org
//! [Tokio]: https://tokio.rs

pub mod command;
pub mod response;

mod connection;
mod parser;

use std::{error::Error, fmt, io};

#[cfg(feature = "async")]
pub use self::connection::AsyncConnection;
pub use self::{
    command::{Command, CommandList},
    connection::Connection,
    response::Response,
};

/// Unrecoverable errors.
#[derive(Debug)]
pub enum MpdProtocolError {
    /// IO error occurred
    Io(io::Error),
    /// A message could not be parsed successfully.
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
