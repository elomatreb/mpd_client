//! Strongly typed, pre-built commands.
//!
//! This module contains pre-made definitions of commands and responses, so you don't have to
//! wrangle the stringly-typed raw responses if you don't want to.
//!
//! The fields on the contained structs are mostly undocumented, see the [MPD protocol
//! documentation][mpd-docs] for details on their specific meaning.
//!
//! [mpd-docs]: https://www.musicpd.org/doc/html/protocol.html#command-reference

pub mod definitions;
pub mod responses;

mod command_list;

use std::{
    error::Error,
    fmt::{self, Write},
    num::{ParseFloatError, ParseIntError},
    time::Duration,
};

use bytes::BytesMut;
use chrono::ParseError;
use mpd_protocol::{
    command::{Argument, Command as RawCommand},
    response::Frame,
};

pub use self::{command_list::CommandList, definitions::*};

/// Stable identifier of a song in the queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SongId(pub u64);

impl From<u64> for SongId {
    fn from(id: u64) -> Self {
        Self(id)
    }
}

impl Argument for SongId {
    fn render(&self, buf: &mut BytesMut) {
        write!(buf, "{}", self.0).unwrap();
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
    fn render(&self, buf: &mut BytesMut) {
        write!(buf, "{}", self.0).unwrap();
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

/// Error returned when failing to convert a raw [`Frame`] into the proper typed response.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypedResponseError {
    pub(crate) field: &'static str,
    pub(crate) kind: ErrorKind,
}

/// Types of parse errors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ErrorKind {
    /// A required field was missing entirely.
    Missing,
    /// We expected a certain field, but found another.
    UnexpectedField(String),
    /// A field had a unexpected value.
    InvalidValue(String),
    /// A field containing an integer failed to parse.
    MalformedInteger(ParseIntError),
    /// A field containing a float (duration) failed to parse.
    MalformedFloat(ParseFloatError),
    /// A field containing a timestamp failed to parse.
    MalformedTimestamp(ParseError),
}

impl fmt::Display for TypedResponseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            ErrorKind::Missing => write!(f, "field {:?} is required but missing", self.field),
            ErrorKind::InvalidValue(val) => {
                write!(f, "value {:?} is invalid for field {:?}", val, self.field)
            }
            ErrorKind::UnexpectedField(found) => {
                write!(f, "expected field {:?} but found {:?}", self.field, found)
            }
            ErrorKind::MalformedInteger(_) => write!(f, "field {:?} is not an integer", self.field),
            ErrorKind::MalformedFloat(_) => {
                write!(f, "field {:?} is not a valid duration", self.field)
            }
            ErrorKind::MalformedTimestamp(_) => {
                write!(f, "field {:?} is not a valid timestamp", self.field)
            }
        }
    }
}

impl Error for TypedResponseError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match &self.kind {
            ErrorKind::MalformedFloat(e) => Some(e),
            ErrorKind::MalformedInteger(e) => Some(e),
            ErrorKind::MalformedTimestamp(e) => Some(e),
            _ => None,
        }
    }
}
