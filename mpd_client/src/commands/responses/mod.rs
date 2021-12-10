//! Typed responses to individual commands.

#[macro_use]
mod util_macros;

mod list;
mod playlist;
mod song;

use bytes::Bytes;
use chrono::ParseError;

use std::error::Error;
use std::fmt;
use std::num::{ParseFloatError, ParseIntError};
use std::sync::Arc;
use std::time::Duration;

use crate::commands::{SingleMode, SongId, SongPosition};
use crate::raw::Frame;
use crate::sealed;

pub use list::List;
pub use playlist::Playlist;
pub use song::{Song, SongInQueue, SongRange};

type KeyValuePair = (Arc<str>, String);

/// "Marker" trait for responses to commands.
///
/// This is sealed, so it cannot be implemented.
pub trait Response: Sized + sealed::Sealed {
    /// Attempt to convert the raw [`Frame`] into the response type.
    #[doc(hidden)]
    fn from_frame(frame: Frame) -> Result<Self, TypedResponseError>;
}

/// Error returned when failing to convert a raw [`Frame`] into the proper typed response.
///
/// [`Frame`]: crate::raw::Frame
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
    /// We expected a certain field, but found another.
    UnexpectedField(String),
    /// A field had a unexpected value.
    InvalidValue(String),
    /// A field containing an integer failed to parse.
    MalformedInteger(ParseIntError),
    /// A field containing a float (duration) failed to parse.
    MalformedFloat(ParseFloatError),
    /// A field containing a duration contained an impossible value (e.g. negative or NaN).
    InvalidTimestamp,
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
            ErrorKind::MalformedFloat(_) | ErrorKind::InvalidTimestamp => {
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

fn parse_duration(field: &'static str, value: &str) -> Result<Duration, TypedResponseError> {
    let value = value.parse::<f64>().map_err(|e| TypedResponseError {
        field,
        kind: ErrorKind::MalformedFloat(e),
    })?;

    // Check if the parsed value is a reasonable duration, to avoid a panic from `from_secs_f64`
    if value >= 0.0 && value <= Duration::MAX.as_secs_f64() && value.is_finite() {
        Ok(Duration::from_secs_f64(value))
    } else {
        Err(TypedResponseError {
            field,
            kind: ErrorKind::InvalidTimestamp,
        })
    }
}

/// An empty response, which only indicates success.
pub type Empty = ();

impl sealed::Sealed for Empty {}
impl Response for Empty {
    fn from_frame(_: Frame) -> Result<Self, TypedResponseError> {
        // silently ignore any actually existing fields
        Ok(())
    }
}

/// Possible playback states.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum PlayState {
    Stopped,
    Playing,
    Paused,
}

/// Response to the [`status`] command.
///
/// See the [MPD documentation][status-command] for the specific meanings of the fields.
///
/// [`status`]: crate::commands::definitions::Status
/// [status-command]: https://www.musicpd.org/doc/html/protocol.html#command-status
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(missing_docs)]
#[non_exhaustive]
pub struct Status {
    pub volume: u8,
    pub state: PlayState,
    pub repeat: bool,
    pub random: bool,
    pub consume: bool,
    pub single: SingleMode,
    pub playlist_version: u32,
    pub playlist_length: usize,
    pub current_song: Option<(SongPosition, SongId)>,
    pub next_song: Option<(SongPosition, SongId)>,
    pub elapsed: Option<Duration>,
    pub duration: Option<Duration>,
    pub bitrate: Option<u64>,
    pub crossfade: Duration,
    pub update_job: Option<u64>,
    pub error: Option<String>,
    /// Name of the non-default partition this client is active on. Will be `None` if the default
    /// partition is active or if the server doesn't send the field at all.
    pub partition: Option<String>,
}

impl sealed::Sealed for Status {}
impl Response for Status {
    fn from_frame(mut raw: Frame) -> Result<Self, TypedResponseError> {
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

        let duration = if let Some(val) = raw.get("duration") {
            Some(parse!(duration, val, "duration"))
        } else if let Some(time) = raw.get("time") {
            // Backwards compatibility with protocol versions <0.20
            if let Some((_, duration)) = time.split_once(':') {
                Some(parse!(duration, duration, "time"))
            } else {
                // No separator
                return Err(TypedResponseError {
                    field: "time",
                    kind: ErrorKind::InvalidValue(time),
                });
            }
        } else {
            None
        };

        let mut partition = raw.get("partition");

        if partition.as_deref() == Some("default") {
            partition = None;
        }

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
            duration,
            bitrate: field!(raw, "bitrate" integer optional),
            crossfade: field!(raw, "xfade" duration default Duration::from_secs(0)),
            update_job: field!(raw, "update_job" integer optional),
            error: raw.get("error"),
            partition,
        })
    }
}

/// Response to the [`stats`] command, containing general server statistics.
///
/// [`stats`]: crate::commands::definitions::Stats
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(missing_docs)]
#[non_exhaustive]
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
    fn from_frame(mut raw: Frame) -> Result<Self, TypedResponseError> {
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

impl sealed::Sealed for Option<SongInQueue> {}
impl Response for Option<SongInQueue> {
    fn from_frame(raw: Frame) -> Result<Self, TypedResponseError> {
        let mut vec = SongInQueue::parse_frame(raw, Some(1))?;
        Ok(vec.pop())
    }
}

impl sealed::Sealed for Vec<SongInQueue> {}
impl Response for Vec<SongInQueue> {
    fn from_frame(raw: Frame) -> Result<Self, TypedResponseError> {
        SongInQueue::parse_frame(raw, None)
    }
}

impl sealed::Sealed for Vec<Song> {}
impl Response for Vec<Song> {
    fn from_frame(raw: Frame) -> Result<Self, TypedResponseError> {
        Song::parse_frame(raw, None)
    }
}

impl sealed::Sealed for SongId {}
impl Response for SongId {
    fn from_frame(mut raw: Frame) -> Result<Self, TypedResponseError> {
        Ok(SongId(field!(raw, "Id" integer)))
    }
}

impl sealed::Sealed for Vec<Playlist> {}
impl Response for Vec<Playlist> {
    fn from_frame(raw: Frame) -> Result<Self, TypedResponseError> {
        let fields_count = raw.fields_len();
        Playlist::parse_frame(raw, fields_count)
    }
}

impl sealed::Sealed for List {}
impl Response for List {
    fn from_frame(frame: Frame) -> Result<Self, TypedResponseError> {
        Ok(List::from_frame(frame))
    }
}

/// Response to the [`albumart`][crate::commands::AlbumArt] and
/// [`readpicture`][crate::commands::AlbumArtEmbedded] commands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AlbumArt {
    /// The total size in bytes of the file.
    pub size: usize,
    /// The mime type, if known.
    pub mime: Option<String>,
    /// The raw data.
    data: Bytes,
}

impl AlbumArt {
    /// Get the data in the response.
    pub fn data(&self) -> &[u8] {
        &self.data
    }
}

impl sealed::Sealed for Option<AlbumArt> {}
impl Response for Option<AlbumArt> {
    fn from_frame(mut frame: Frame) -> Result<Self, TypedResponseError> {
        let data = match frame.get_binary() {
            Some(d) => d.freeze(),
            None => return Ok(None),
        };

        Ok(Some(AlbumArt {
            size: field!(frame, "size" integer),
            mime: frame.get("type"),
            data,
        }))
    }
}
