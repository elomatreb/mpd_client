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

mod command_list;

use std::{fmt::Write, time::Duration};

use bytes::BytesMut;
use mpd_protocol::{
    command::{Argument, Command as RawCommand},
    response::Frame,
};

pub use self::{command_list::CommandList, definitions::*};
use crate::responses::TypedResponseError;

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

/// Possible ways to list files, directories and playlists
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListItemsMode<'a> {
    /// Lists all songs and directories in URI.
    ListAll(&'a str),
    /// Same as listall, except it also returns metadata info in the same format as lsinfo
    ListAllInfo(&'a str),
    /// Lists the contents of the directory URI, including files are not recognized by MPD
    ListFiles(&'a str),
    /// Lists the contents of the directory URI.
    LsInfo(&'a str),
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

/// Possible `replay_gain_mode` modes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum ReplayGainMode {
    /// Replay Gain off
    Off,
    /// Replay Gain Track mode
    Track,
    /// Replay Gain Album mode
    Album,
    /// Replay Gain Track if shuffle is on, Album otherwise
    Auto,
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
    /// The response this command will return.
    type Response;

    /// Create the raw command representation for transmission.
    fn command(&self) -> RawCommand;

    /// Convert the raw response frame to the proper response type.
    ///
    /// # Errors
    ///
    /// This should return an error if the response was invalid.
    fn response(self, frame: Frame) -> Result<Self::Response, TypedResponseError>;
}
