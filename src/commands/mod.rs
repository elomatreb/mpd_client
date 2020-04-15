//! Strongly typed, pre-built commands.
//!
//! This module contains pre-made definitions of commands and responses, so you don't have to
//! wrangle the stringly-typed raw responses if you don't want to.

pub mod definitions;
pub mod responses;

use mpd_protocol::response::Frame;

use std::convert::TryFrom;

use responses::TypedResponseError;
pub use definitions::*;

/// Types which can be used as pre-built properly typed commands.
pub trait Command {
    /// The response this command expects.
    type Response: TryFrom<Frame, Error = TypedResponseError>;

    /// Create the "raw" command representation for transmission.
    fn to_command(self) -> mpd_protocol::Command;
}

