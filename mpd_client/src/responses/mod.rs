//! Typed responses to individual commands.

mod count;
mod list;
mod playlist;
mod song;
mod sticker;
mod timestamp;
mod storage_item;

use std::{error::Error, fmt, num::ParseIntError, str::FromStr, sync::Arc, time::Duration};

use bytes::BytesMut;
use mpd_protocol::response::Frame;

pub use self::{
    count::Count,
    list::{GroupedListValuesIter, List, ListValuesIntoIter, ListValuesIter},
    playlist::Playlist,
    song::{Song, SongInQueue, SongRange},
    storage_item:: StorageItem,
    sticker::{StickerFind, StickerGet, StickerList},
    timestamp::Timestamp,
};
use crate::commands::{ReplayGainMode, SingleMode, SongId, SongPosition};

type KeyValuePair = (Arc<str>, String);

/// Error returned when failing to convert a raw [`Frame`] into the proper typed response.
#[derive(Debug)]
pub struct TypedResponseError {
    kind: ErrorKind,
    source: Option<Box<dyn Error + Send + Sync>>,
}

impl TypedResponseError {
    /// Construct a "Missing field" error.
    pub fn missing<F>(field: F) -> TypedResponseError
    where
        F: Into<String>,
    {
        TypedResponseError {
            kind: ErrorKind::Missing {
                field: field.into(),
            },
            source: None,
        }
    }

    /// Construct an "Unexpected field" error.
    pub fn unexpected_field<E, F>(expected: E, found: F) -> TypedResponseError
    where
        E: Into<String>,
        F: Into<String>,
    {
        TypedResponseError {
            kind: ErrorKind::UnexpectedField {
                expected: expected.into(),
                found: found.into(),
            },
            source: None,
        }
    }

    /// Construct an "Invalid value" error.
    pub fn invalid_value<F>(field: F, value: String) -> TypedResponseError
    where
        F: Into<String>,
    {
        TypedResponseError {
            kind: ErrorKind::InvalidValue {
                field: field.into(),
                value,
            },
            source: None,
        }
    }

    /// Construct a nonspecific error.
    pub fn other() -> TypedResponseError {
        TypedResponseError {
            kind: ErrorKind::Other,
            source: None,
        }
    }

    /// Set a source error.
    ///
    /// This is most useful with [invalid value][TypedResponseError::invalid_value] and
    /// [unspecified][TypedResponseError::other] errors.
    pub fn source<E>(self, source: E) -> TypedResponseError
    where
        E: Error + Send + Sync + 'static,
    {
        let source = Some(Box::from(source));
        TypedResponseError { source, ..self }
    }
}

#[derive(Debug)]
enum ErrorKind {
    Missing { field: String },
    UnexpectedField { expected: String, found: String },
    InvalidValue { field: String, value: String },
    Other,
}

impl fmt::Display for TypedResponseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            ErrorKind::Missing { field } => write!(f, "field {field:?} is required but missing"),
            ErrorKind::UnexpectedField { expected, found } => {
                write!(f, "expected field {expected:?} but found {found:?}")
            }
            ErrorKind::InvalidValue { field, value } => {
                write!(f, "invalid value {value:?} for field {field:?}")
            }
            ErrorKind::Other => write!(f, "invalid response"),
        }
    }
}

impl Error for TypedResponseError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source.as_deref().map(|e| e as _)
    }
}

/// Types which can be converted from a field value.
pub(crate) trait FromFieldValue: Sized {
    /// Convert the value.
    fn from_value(v: String, field: &str) -> Result<Self, TypedResponseError>;
}

impl FromFieldValue for bool {
    fn from_value(v: String, field: &str) -> Result<Self, TypedResponseError> {
        match &*v {
            "0" => Ok(false),
            "1" => Ok(true),
            _ => Err(TypedResponseError::invalid_value(field, v)),
        }
    }
}

impl FromFieldValue for Duration {
    fn from_value(v: String, field: &str) -> Result<Self, TypedResponseError> {
        parse_duration(field, v)
    }
}

impl FromFieldValue for PlayState {
    fn from_value(v: String, field: &str) -> Result<Self, TypedResponseError> {
        match &*v {
            "play" => Ok(PlayState::Playing),
            "pause" => Ok(PlayState::Paused),
            "stop" => Ok(PlayState::Stopped),
            _ => Err(TypedResponseError::invalid_value(field, v)),
        }
    }
}

impl FromFieldValue for ReplayGainMode {
    fn from_value(v: String, field: &str) -> Result<Self, TypedResponseError> {
        match &*v {
            "off" => Ok(ReplayGainMode::Off),
            "track" => Ok(ReplayGainMode::Track),
            "album" => Ok(ReplayGainMode::Album),
            "auto" => Ok(ReplayGainMode::Auto),
            _ => Err(TypedResponseError::invalid_value(field, v)),
        }
    }
}

fn parse_integer<I: FromStr<Err = ParseIntError>>(
    v: String,
    field: &str,
) -> Result<I, TypedResponseError> {
    v.parse::<I>()
        .map_err(|e| TypedResponseError::invalid_value(field, v).source(e))
}

impl FromFieldValue for u8 {
    fn from_value(v: String, field: &str) -> Result<Self, TypedResponseError> {
        parse_integer(v, field)
    }
}

impl FromFieldValue for u32 {
    fn from_value(v: String, field: &str) -> Result<Self, TypedResponseError> {
        parse_integer(v, field)
    }
}

impl FromFieldValue for u64 {
    fn from_value(v: String, field: &str) -> Result<Self, TypedResponseError> {
        parse_integer(v, field)
    }
}

impl FromFieldValue for usize {
    fn from_value(v: String, field: &str) -> Result<Self, TypedResponseError> {
        parse_integer(v, field)
    }
}

/// Get a *required* value for the given field, as the given type.
pub(crate) fn value<V: FromFieldValue>(
    frame: &mut Frame,
    field: &'static str,
) -> Result<V, TypedResponseError> {
    let value = frame
        .get(field)
        .ok_or_else(|| TypedResponseError::missing(field))?;
    V::from_value(value, field)
}

/// Get an *optional* value for the given field, as the given type.
fn optional_value<V: FromFieldValue>(
    frame: &mut Frame,
    field: &'static str,
) -> Result<Option<V>, TypedResponseError> {
    match frame.get(field) {
        None => Ok(None),
        Some(v) => {
            let v = V::from_value(v, field)?;
            Ok(Some(v))
        }
    }
}

fn song_identifier(
    frame: &mut Frame,
    position_field: &'static str,
    id_field: &'static str,
) -> Result<Option<(SongPosition, SongId)>, TypedResponseError> {
    // The position field may or may not exist
    let position = match optional_value(frame, position_field)? {
        Some(p) => SongPosition(p),
        None => return Ok(None),
    };

    // ... but if the position field existed, the ID field must exist too
    let id = value(frame, id_field).map(SongId)?;

    Ok(Some((position, id)))
}

fn parse_duration<V: AsRef<str> + Into<String>>(
    field: &str,
    value: V,
) -> Result<Duration, TypedResponseError> {
    let v = match value.as_ref().parse::<f64>() {
        Ok(v) => v,
        Err(e) => return Err(TypedResponseError::invalid_value(field, value.into()).source(e)),
    };

    // Check if the parsed value is a reasonable duration, to avoid a panic from `from_secs_f64`
    if v >= 0.0 && v <= Duration::MAX.as_secs_f64() && v.is_finite() {
        Ok(Duration::from_secs_f64(v))
    } else {
        Err(TypedResponseError::invalid_value(field, value.into()))
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

/// Response to the [`replay_gain_status`] command.
///
/// See the [MPD documentation][replay-gain-status-command] for the specific meanings of the fields.
///
/// [`replay_gain_status`]: crate::commands::definitions::ReplayGainStatus
/// [replay-gain-status-command]: https://www.musicpd.org/doc/html/protocol.html#command-replay-gain-status
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(missing_docs)]
#[non_exhaustive]
pub struct ReplayGainStatus {
    pub mode: ReplayGainMode,
}

impl ReplayGainStatus {
    pub(crate) fn from_frame(mut raw: Frame) -> Result<Self, TypedResponseError> {
        let f = &mut raw;
        Ok(Self {
            mode: value(f, "replay_gain_mode")?,
        })
    }
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
                _ => return Err(TypedResponseError::invalid_value("single", val)),
            },
        };

        let duration = if let Some(val) = raw.get("duration") {
            Some(Duration::from_value(val, "duration")?)
        } else if let Some(time) = raw.get("Time") {
            // Backwards compatibility with protocol versions <0.20
            if let Some((_, duration)) = time.split_once(':') {
                Some(Duration::from_value(duration.to_owned(), "Time")?)
            } else {
                // No separator
                return Err(TypedResponseError::invalid_value("Time", time));
            }
        } else {
            None
        };

        let f = &mut raw;

        Ok(Self {
            volume: optional_value(f, "volume")?.unwrap_or(0),
            state: value(f, "state")?,
            repeat: value(f, "repeat")?,
            random: value(f, "random")?,
            consume: value(f, "consume")?,
            single,
            playlist_length: optional_value(f, "playlistlength")?.unwrap_or(0),
            playlist_version: optional_value(f, "playlist")?.unwrap_or(0),
            current_song: song_identifier(f, "song", "songid")?,
            next_song: song_identifier(f, "nextsong", "nextsongid")?,
            elapsed: optional_value(f, "elapsed")?,
            duration,
            bitrate: optional_value(f, "bitrate")?,
            crossfade: optional_value(f, "xfade")?.unwrap_or(Duration::ZERO),
            update_job: optional_value(f, "update_job")?,
            error: f.get("error"),
            partition: f.get("partition"),
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
    pub(crate) fn from_frame(mut f: Frame) -> Result<Self, TypedResponseError> {
        let f = &mut f;
        Ok(Self {
            artists: value(f, "artists")?,
            albums: value(f, "albums")?,
            songs: value(f, "songs")?,
            uptime: value(f, "uptime")?,
            playtime: value(f, "playtime")?,
            db_playtime: value(f, "db_playtime")?,
            db_last_update: value(f, "db_update")?,
        })
    }
}

/// Response to the [`listparitions`] command, containing list of all partitions.
///
/// [`partition`]: crate::commands::definitions::SwitchPartition
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(missing_docs)]
#[non_exhaustive]
pub struct Partition {
    pub name: String
}

impl Partition {
    pub(crate) fn from_frame_multi(f: Frame) -> Result<Vec<Partition>, TypedResponseError> {
        let mut out = Vec::new();

        f.fields().for_each(|(_, value)| {
            out.push(Partition{ name: value.to_string() });
        });

        Ok(out)
    }
}

/// Response to the [`albumart`][crate::commands::AlbumArt] and
/// [`readpicture`][crate::commands::AlbumArtEmbedded] commands.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AlbumArt {
    /// The total size in bytes of the file.
    pub size: usize,
    /// The mime type, if known.
    pub mime: Option<String>,
    /// The raw data.
    pub data: BytesMut,
}

impl AlbumArt {
    pub(crate) fn from_frame(mut frame: Frame) -> Result<Option<Self>, TypedResponseError> {
        let Some(data) = frame.take_binary() else {
            return Ok(None);
        };

        Ok(Some(AlbumArt {
            size: value(&mut frame, "size")?,
            mime: frame.get("type"),
            data,
        }))
    }
}

/// Parse response for the [`crate::commands::ReadChannelMessages`] command.
pub(crate) fn parse_channel_messages<F>(
    fields: F,
) -> Result<Vec<(String, String)>, TypedResponseError>
where
    F: IntoIterator<Item = KeyValuePair>,
{
    let mut response = Vec::new();
    let mut fields = fields.into_iter();

    while let Some(channel) = fields.next() {
        if &*channel.0 != "channel" {
            return Err(TypedResponseError::unexpected_field("channel", &*channel.0));
        }

        let Some(message) = fields.next() else {
            return Err(TypedResponseError::missing("message"));
        };

        if &*message.0 != "message" {
            return Err(TypedResponseError::unexpected_field("message", &*message.0));
        }

        response.push((channel.1, message.1));
    }

    Ok(response)
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;

    use super::*;

    #[test]
    fn duration_parsing() {
        assert_eq!(
            parse_duration("duration", "1.500").unwrap(),
            Duration::from_secs_f64(1.5)
        );
        assert_eq!(parse_duration("Time", "3").unwrap(), Duration::from_secs(3));

        // Error cases
        assert_matches!(parse_duration("duration", "asdf"), Err(_));
        assert_matches!(parse_duration("duration", "-1"), Err(_));
        assert_matches!(parse_duration("duration", "NaN"), Err(_));
        assert_matches!(parse_duration("duration", "-1"), Err(_));
    }

    #[test]
    fn channel_message_parsing() {
        assert_eq!(parse_channel_messages(Vec::new()).unwrap(), Vec::new());

        let fields = vec![
            (Arc::from("channel"), String::from("foo")),
            (Arc::from("message"), String::from("message 1")),
            (Arc::from("channel"), String::from("bar")),
            (Arc::from("message"), String::from("message 2")),
        ];
        assert_eq!(
            parse_channel_messages(fields).unwrap(),
            vec![
                (String::from("foo"), String::from("message 1")),
                (String::from("bar"), String::from("message 2")),
            ]
        );
    }
}
