//! Typed responses to individual commands.

use mpd_protocol::response::Frame;

use std::convert::TryFrom;
use std::error::Error;
use std::fmt;
use std::num::{ParseFloatError, ParseIntError};
use std::time::Duration;

/// Error returned when failing to convert a raw `Frame` into the proper typed response.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(missing_copy_implementations)]
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

impl TryFrom<Frame> for Empty {
    type Error = TypedResponseError;

    fn try_from(_: Frame) -> Result<Self, Self::Error> {
        // silently ignore any actually existing fields
        Ok(Self)
    }
}

macro_rules! field {
    ($frame:ident, $field:literal $type:ident) => {
        field!($frame, $field $type optional)
            .ok_or(TypedResponseError {
                field: $field,
                kind: ErrorKind::Missing,
            })?
    };
    ($frame:ident, $field:literal $type:ident optional) => {
        match $frame.get($field) {
            None => None,
            Some(val) => Some(parse!($type, val, $field))
        }
    };
    ($frame:ident, $field:literal $type:ident default $default:expr) => {
        field!($frame, $field $type optional).unwrap_or($default)
    };
}

macro_rules! parse {
    (integer, $value:ident, $field:literal) => {
        $value.parse().map_err(|e| TypedResponseError {
            field: $field,
            kind: ErrorKind::MalformedInteger(e),
        })?
    };
    (PlayState, $value:ident, $field:literal) => {
        match $value.as_str() {
            "play" => PlayState::Playing,
            "pause" => PlayState::Paused,
            "stop" => PlayState::Stopped,
            _ => {
                return Err(TypedResponseError {
                    field: $field,
                    kind: ErrorKind::InvalidValue($value),
                })
            }
        }
    };
    (boolean, $value:ident, $field:literal) => {
        match $value.as_str() {
            "1" => true,
            "0" => false,
            _ => {
                return Err(TypedResponseError {
                    field: $field,
                    kind: ErrorKind::InvalidValue($value),
                })
            }
        }
    };
    (duration, $value:ident, $field:literal) => {
        Duration::from_secs_f64($value.parse().map_err(|e| TypedResponseError {
            field: $field,
            kind: ErrorKind::MalformedFloat(e),
        })?)
    };
}

macro_rules! song_identifier {
    ($frame:ident, $position:literal, $id:literal) => {
        {
            let pos = field!($frame, $position integer optional);
            let id = field!($frame, $id integer optional);

            match (pos, id) {
                (Some(pos), Some(id)) => Some(SongIdentifier { pos, id }),
                _ => None,
            }
        }
    };
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

impl TryFrom<Frame> for Status {
    type Error = TypedResponseError;

    fn try_from(mut raw: Frame) -> Result<Self, Self::Error> {
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
