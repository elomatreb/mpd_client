//! Typed responses to individual commands.

#[macro_use]
mod util_macros;

use mpd_protocol::response::Frame;

use std::error::Error;
use std::fmt;
use std::num::{ParseFloatError, ParseIntError};
use std::time::Duration;

use crate::sealed;

/// "Marker" trait for responses to commands.
///
/// This is sealed, so it cannot be implemented.
pub trait Response: Sized + sealed::Sealed {
    #[doc(hidden)]
    fn convert(frame: Frame) -> Result<Self, TypedResponseError>;
}

/// Error returned when failing to convert a raw `Frame` into the proper typed response.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypedResponseError {
    field: &'static str,
    kind: ErrorKind,
}

/// Types of parse errors.
#[derive(Clone, Debug, PartialEq, Eq)]
enum ErrorKind {
    /// A required field was missing entirely.
    Missing,
    /// A field had a unexpected value.
    InvalidValue(String),
    /// A field containing an integer failed to parse.
    MalformedInteger(ParseIntError),
    /// A field containing a float (duration) failed to parse.
    MalformedFloat(ParseFloatError),
}

impl fmt::Display for TypedResponseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Error converting response: ")?;

        match &self.kind {
            ErrorKind::Missing => write!(f, "field {:?} is but missing", self.field),
            ErrorKind::InvalidValue(val) => {
                write!(f, "value {:?} is invalid for field {:?}", val, self.field)
            }
            ErrorKind::MalformedInteger(_) => write!(f, "field {:?} is not an integer", self.field),
            ErrorKind::MalformedFloat(_) => write!(f, "field {:?} is not a float", self.field),
        }
    }
}

impl Error for TypedResponseError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match &self.kind {
            ErrorKind::MalformedFloat(e) => Some(e),
            ErrorKind::MalformedInteger(e) => Some(e),
            _ => None,
        }
    }
}

/// Type of song IDs.
pub type SongId = u64;

/// Type of Job IDs.
pub type JobId = u64;

/// An empty response, which only indicates success.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Empty;

impl sealed::Sealed for Empty {}
impl Response for Empty {
    fn convert(_: Frame) -> Result<Self, TypedResponseError> {
        // silently ignore any actually existing fields
        Ok(Self)
    }
}

/// Identifier for a song in the queue, consisting of position-dependent index and stable ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(missing_docs)]
pub struct SongIdentifier {
    pub pos: usize,
    pub id: SongId,
}

/// Possible playback states.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum PlayState {
    Stopped,
    Playing,
    Paused,
}

/// Possible `single` modes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum SingleMode {
    Enabled,
    Disabled,
    Oneshot,
}

/// Response to the `status` command.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(missing_docs)]
pub struct Status {
    pub volume: u8,
    pub state: PlayState,
    pub repeat: bool,
    pub random: bool,
    pub consume: bool,
    pub single: SingleMode,
    pub playlist_version: u32,
    pub playlist_length: usize,
    pub current_song: Option<SongIdentifier>,
    pub next_song: Option<SongIdentifier>,
    pub elapsed: Option<Duration>,
    pub duration: Option<Duration>,
    pub bitrate: Option<u64>,
    pub crossfade: Duration,
    pub update_job: Option<JobId>,
    pub error: Option<String>,
}

impl sealed::Sealed for Status {}
impl Response for Status {
    fn convert(mut raw: Frame) -> Result<Self, TypedResponseError> {
        let single = match raw.get("single") {
            None => SingleMode::Disabled,
            Some(val) => match val.as_str() {
                "0" => SingleMode::Disabled,
                "1" => SingleMode::Enabled,
                "oneshot" => SingleMode::Oneshot,
                _ => {
                    return Err(TypedResponseError {
                        field: "single",
                        kind: ErrorKind::InvalidValue(val),
                    })
                }
            },
        };

        Ok(Self {
            volume: field!(raw, "volume" integer default 0),
            state: field!(raw, "state" PlayState),
            repeat: field!(raw, "repeat" boolean),
            random: field!(raw, "random" boolean),
            consume: field!(raw, "consume" boolean),
            single,
            playlist_length: field!(raw, "playlistlength" integer default 0),
            playlist_version: field!(raw, "playlist" integer default 0),
            current_song: song_identifier!(raw, "song", "songid"),
            next_song: song_identifier!(raw, "nextsong", "nextsongid"),
            elapsed: field!(raw, "elapsed" duration optional),
            duration: field!(raw, "duration" duration optional),
            bitrate: field!(raw, "bitrate" integer optional),
            crossfade: field!(raw, "xfade" duration default Duration::from_secs(0)),
            update_job: field!(raw, "update_job" integer optional),
            error: raw.get("error"),
        })
    }
}

/// Response to the `stats` command.
///
/// General server statistics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(missing_docs)]
pub struct Stats {
    pub artists: u64,
    pub albums: u64,
    pub songs: u64,
    pub uptime: Duration,
    pub playtime: Duration,
    pub db_playtime: Duration,
    /// Raw server UNIX timestamp of last database update.
    pub db_last_update: u64,
}

impl sealed::Sealed for Stats {}
impl Response for Stats {
    fn convert(mut raw: Frame) -> Result<Self, TypedResponseError> {
        Ok(Self {
            artists: field!(raw, "artists" integer),
            albums: field!(raw, "albums" integer),
            songs: field!(raw, "songs" integer),
            uptime: field!(raw, "uptime" duration),
            playtime: field!(raw, "playtime" duration),
            db_playtime: field!(raw, "db_playtime" duration),
            db_last_update: field!(raw, "db_update" integer),
        })
    }
}
