//! Strongly typed, pre-built commands.
//!
//! This module contains pre-made definitions of commands and responses, so you don't have to
//! wrangle the stringly-typed raw responses if you don't want to.
//!
//! The fields on the contained structs are mostly undocumented, see the [MPD protocol
//! documentation][mpd-docs] for details on their specific meaning.
//!
//! [mpd-docs]: https://www.musicpd.org/doc/html/protocol.html#command-reference

#[macro_use]
mod util_macros;

pub mod definitions;
pub mod responses;

mod command_list;

use std::borrow::Cow;
use std::time::Duration;

use mpd_protocol::{command::Argument, response::Frame};

use crate::errors::TypedResponseError;
use crate::raw::RawCommand;

pub use command_list::CommandList;
pub use definitions::*;

/// Stable identifier of a song in the queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SongId(pub u64);

impl From<u64> for SongId {
    fn from(id: u64) -> Self {
        Self(id)
    }
}

impl Argument for SongId {
    fn render(self) -> Cow<'static, str> {
        Cow::Owned(self.0.to_string())
    }
}

/// Position of a song in the queue.
///
/// This will change when the queue is modified.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SongPosition(pub usize);

impl From<usize> for SongPosition {
    fn from(pos: usize) -> Self {
        Self(pos)
    }
}

impl Argument for SongPosition {
    fn render(self) -> Cow<'static, str> {
        Cow::Owned(self.0.to_string())
    }
}

/// Possible ways to seek in the current song.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeekMode {
    /// Forwards from current position.
    Forward(Duration),
    /// Backwards from current position.
    Backward(Duration),
    /// To the absolute position in the current song.
    Absolute(Duration),
}

/// Possible `single` modes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum SingleMode {
    Enabled,
    Disabled,
    Oneshot,
}

/// Modes to target a song with a command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Song {
    /// By ID
    Id(SongId),
    /// By position in the queue.
    Position(SongPosition),
}

impl From<SongId> for Song {
    fn from(id: SongId) -> Self {
        Self::Id(id)
    }
}

impl From<SongPosition> for Song {
    fn from(pos: SongPosition) -> Self {
        Self::Position(pos)
    }
}

/// Types which can be used as pre-built properly typed commands.
pub trait Command {
    /// The response this command expects.
    type Response;

    /// Create the "raw" command representation for transmission.
    fn command(&self) -> RawCommand;

    /// Create the response type from the raw response.
    fn response(self, frame: Frame) -> Result<Self::Response, TypedResponseError>;
}
