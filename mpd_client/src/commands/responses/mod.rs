//! Typed responses to individual commands.

mod list;
mod playlist;
mod song;
mod sticker;

use bytes::Bytes;

use std::sync::Arc;
use std::time::Duration;

use crate::commands::{SingleMode, SongId, SongPosition};
use crate::errors::{ErrorKind, TypedResponseError};
use crate::raw::Frame;

pub use list::List;
pub use playlist::Playlist;
pub use song::{Song, SongInQueue, SongRange};
pub use sticker::{StickerFind, StickerGet, StickerList};

type KeyValuePair = (Arc<str>, String);

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

impl Status {
    pub(crate) fn from_frame(mut raw: Frame) -> Result<Self, TypedResponseError> {
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

impl Stats {
    pub(crate) fn from_frame(mut raw: Frame) -> Result<Self, TypedResponseError> {
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

    pub(crate) fn from_frame(mut frame: Frame) -> Result<Option<Self>, TypedResponseError> {
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
