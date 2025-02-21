use std::{collections::HashMap, mem, path::Path, time::Duration};

use mpd_protocol::response::Frame;

use crate::{
    commands::{SongId, SongPosition},
    responses::{FromFieldValue, Timestamp, TypedResponseError, parse_duration},
    tag::Tag,
};

/// A [`Song`] in the current queue, as returned by the [`playlistinfo`] command.
///
/// [`playlistinfo`]: crate::commands::definitions::Queue
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SongInQueue {
    /// Position in queue.
    pub position: SongPosition,
    /// ID in queue.
    pub id: SongId,
    /// The range of the song that will be played.
    pub range: Option<SongRange>,
    /// The priority.
    pub priority: u8,
    /// The song.
    pub song: Song,
}

impl SongInQueue {
    /// Convert the given frame into a single `SongInQueue`.
    pub(crate) fn from_frame_single(
        frame: Frame,
    ) -> Result<Option<SongInQueue>, TypedResponseError> {
        let mut builder = SongBuilder::default();

        for (key, value) in frame {
            builder.field(&key, value)?;
        }

        Ok(builder.finish())
    }

    /// Convert the given frame into a list of `SongInQueue`s.
    pub(crate) fn from_frame_multi(frame: Frame) -> Result<Vec<SongInQueue>, TypedResponseError> {
        let mut out = Vec::new();
        let mut builder = SongBuilder::default();

        for (key, value) in frame {
            if let Some(song) = builder.field(&key, value)? {
                out.push(song);
            }
        }

        if let Some(song) = builder.finish() {
            out.push(song);
        }

        Ok(out)
    }
}

/// A single song, as returned by the [playlist] or [current song] commands.
///
/// [playlist]: crate::commands::definitions::Queue
/// [current song]: crate::commands::definitions::CurrentSong
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Song {
    /// Unique identifier of the song. May be a file path relative to the library root, or an URL
    /// to a remote resource.
    ///
    /// This is the `file` key as returned by MPD.
    pub url: String,
    /// The `duration` as returned by MPD.
    pub duration: Option<Duration>,
    /// Tags in this response.
    pub tags: HashMap<Tag, Vec<String>>,
    /// The `format` as returned by MPD.
    pub format: Option<String>,
    /// Last modification date of the underlying file.
    pub last_modified: Option<Timestamp>,
}

impl Song {
    /// Get the file as a `Path`. Note that if the file is a remote URL, operations on the result
    /// will give unexpected results.
    pub fn file_path(&self) -> &Path {
        Path::new(&self.url)
    }

    /// Get all artists of the song.
    pub fn artists(&self) -> &[String] {
        self.tag_values(&Tag::Artist)
    }

    /// Get all album artists of the song.
    pub fn album_artists(&self) -> &[String] {
        self.tag_values(&Tag::AlbumArtist)
    }

    /// Get the album of the song.
    pub fn album(&self) -> Option<&str> {
        self.single_tag_value(&Tag::Album)
    }

    /// Get the title of the song.
    pub fn title(&self) -> Option<&str> {
        self.single_tag_value(&Tag::Title)
    }

    /// Get the disc and track number of the song.
    ///
    /// If either are not set on the song, 0 is returned. This is a utility for sorting.
    pub fn number(&self) -> (u64, u64) {
        let disc = self.single_tag_value(&Tag::Disc);
        let track = self.single_tag_value(&Tag::Track);

        (
            disc.and_then(|v| v.parse().ok()).unwrap_or(0),
            track.and_then(|v| v.parse().ok()).unwrap_or(0),
        )
    }

    /// Convert the given frame into a list of `Song`s.
    pub(crate) fn from_frame_multi(frame: Frame) -> Result<Vec<Song>, TypedResponseError> {
        let mut out = Vec::new();
        let mut builder = SongBuilder::default();

        for (key, value) in frame {
            if let Some(SongInQueue { song, .. }) = builder.field(&key, value)? {
                out.push(song);
            }
        }

        if let Some(SongInQueue { song, .. }) = builder.finish() {
            out.push(song);
        }

        Ok(out)
    }

    fn tag_values(&self, tag: &Tag) -> &[String] {
        match self.tags.get(tag) {
            Some(v) => v.as_slice(),
            None => &[],
        }
    }

    fn single_tag_value(&self, tag: &Tag) -> Option<&str> {
        match self.tag_values(tag) {
            [] => None,
            [v, ..] => Some(v),
        }
    }
}

/// Range used when playing only part of a [`Song`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SongRange {
    /// Start playback at this timestamp.
    pub from: Duration,
    /// End at this timestamp (if the end is known).
    pub to: Option<Duration>,
}

impl FromFieldValue for SongRange {
    fn from_value(v: String, field: &str) -> Result<Self, TypedResponseError> {
        // The range follows the form "<start>-<end?>"
        let Some((from, to)) = v.split_once('-') else {
            return Err(TypedResponseError::invalid_value(field, v));
        };

        let from = parse_duration(field, from)?;

        let to = if to.is_empty() {
            None
        } else {
            Some(parse_duration(field, to)?)
        };

        Ok(SongRange { from, to })
    }
}

#[derive(Debug, Default)]
struct SongBuilder {
    url: String,
    position: usize,
    id: u64,
    range: Option<SongRange>,
    priority: u8,
    duration: Option<Duration>,
    tags: HashMap<Tag, Vec<String>>,
    format: Option<String>,
    last_modified: Option<Timestamp>,
}

impl SongBuilder {
    /// Handle a field from a song list.
    ///
    /// If this returns `Ok(Some(_))`, a song was completed and another one started.
    fn field(
        &mut self,
        key: &str,
        value: String,
    ) -> Result<Option<SongInQueue>, TypedResponseError> {
        if self.url.is_empty() {
            // No song is currently in progress
            self.handle_start_field(key, value)?;
            Ok(None)
        } else {
            // Currently parsing a song
            self.handle_song_field(key, value)
        }
    }

    /// Handle a field that is expected to start a new song.
    fn handle_start_field(&mut self, key: &str, value: String) -> Result<(), TypedResponseError> {
        match key {
            // A `file` field starts a new song
            "file" => self.url = value,
            // Skip directory or playlist entries, encountered when using commands like
            // `listallinfo`, as well as the last modified date associated with these entries
            "directory" | "playlist" | "Last-Modified" => (),
            // Any other fields are invalid
            other => return Err(TypedResponseError::unexpected_field("file", other)),
        }

        Ok(())
    }

    /// Handle a field that may be part of a song or may start a new one.
    fn handle_song_field(
        &mut self,
        key: &str,
        value: String,
    ) -> Result<Option<SongInQueue>, TypedResponseError> {
        // If this field starts a new song, the current one is done
        if is_start_field(key) {
            // Reset the song builder and convert the existing data into a song
            let song = mem::take(self).into_song();

            // Handle the current field
            self.handle_start_field(key, value)?;

            // Return the complete song
            return Ok(Some(song));
        }

        // The field is a component of a song
        match key {
            "duration" => self.duration = Some(Duration::from_value(value, "duration")?),
            "Time" => {
                // Just a worse `duration` field, but retained for backwards compatibility with
                // protocol versions <0.20
                if self.duration.is_none() {
                    self.duration = Some(Duration::from_value(value, "Time")?);
                }
            }
            "Range" => self.range = Some(SongRange::from_value(value, "Range")?),
            "Format" => self.format = Some(value),
            "Last-Modified" => {
                let lm = Timestamp::from_value(value, "Last-Modified")?;
                self.last_modified = Some(lm);
            }
            "Prio" => self.priority = u8::from_value(value, "Prio")?,
            "Pos" => self.position = usize::from_value(value, "Pos")?,
            "Id" => self.id = u64::from_value(value, "Id")?,
            tag => {
                // Anything else is a tag.
                // It's fine to unwrap here because the protocol implementation already validated
                // the field name
                let tag = Tag::try_from(tag).unwrap();
                self.tags.entry(tag).or_default().push(value);
            }
        }

        Ok(None)
    }

    /// Finish the building process. This returns the final song, if there is one.
    fn finish(self) -> Option<SongInQueue> {
        if self.url.is_empty() {
            None
        } else {
            Some(self.into_song())
        }
    }

    fn into_song(self) -> SongInQueue {
        assert!(!self.url.is_empty());

        SongInQueue {
            position: SongPosition(self.position),
            id: SongId(self.id),
            range: self.range,
            priority: self.priority,
            song: Song {
                url: self.url,
                duration: self.duration,
                tags: self.tags,
                format: self.format,
                last_modified: self.last_modified,
            },
        }
    }
}

/// Returns `true` if the given field name starts a new song entry.
fn is_start_field(f: &str) -> bool {
    matches!(f, "file" | "directory" | "playlist")
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;

    use super::*;

    const TEST_TIMESTAMP: &str = "2020-06-12T17:53:00Z";

    #[test]
    fn song_builder() {
        let mut builder = SongBuilder::default();

        assert_matches!(builder.field("file", String::from("test.flac")), Ok(None));
        assert_matches!(builder.field("duration", String::from("123.456")), Ok(None));
        assert_matches!(
            builder.field("Last-Modified", String::from(TEST_TIMESTAMP)),
            Ok(None)
        );
        assert_matches!(builder.field("Title", String::from("Foo")), Ok(None));
        assert_matches!(builder.field("Id", String::from("12")), Ok(None));
        assert_matches!(builder.field("Pos", String::from("5")), Ok(None));

        let song = builder
            .field("file", String::from("foo.flac"))
            .unwrap()
            .unwrap();

        assert_eq!(
            song,
            SongInQueue {
                position: SongPosition(5),
                id: SongId(12),
                priority: 0,
                range: None,
                song: Song {
                    url: String::from("test.flac"),
                    duration: Some(Duration::from_secs_f64(123.456)),
                    format: None,
                    last_modified: Some(Timestamp::from_value(TEST_TIMESTAMP.into(), "").unwrap()),
                    tags: [(Tag::Title, vec![String::from("Foo")])].into(),
                }
            }
        );

        let song = builder.finish().unwrap();

        assert_eq!(
            song,
            SongInQueue {
                position: SongPosition(0),
                id: SongId(0),
                priority: 0,
                range: None,
                song: Song {
                    url: String::from("foo.flac"),
                    duration: None,
                    format: None,
                    last_modified: None,
                    tags: HashMap::new(),
                }
            }
        );
    }

    #[test]
    fn song_builder_unrelated_entries() {
        let mut builder = SongBuilder::default();

        assert_matches!(builder.field("playlist", String::from("foo.m3u")), Ok(None));
        assert_matches!(builder.field("directory", String::from("foo")), Ok(None));
        assert_matches!(
            builder.field("Last-Modified", String::from(TEST_TIMESTAMP)),
            Ok(None)
        );
        assert_matches!(builder.field("file", String::from("foo.flac")), Ok(None));

        let song = builder
            .field("directory", String::from("mep"))
            .unwrap()
            .unwrap();

        assert_eq!(
            song,
            SongInQueue {
                position: SongPosition(0),
                id: SongId(0),
                priority: 0,
                range: None,
                song: Song {
                    url: String::from("foo.flac"),
                    duration: None,
                    format: None,
                    last_modified: None,
                    tags: HashMap::new(),
                }
            }
        );

        assert_matches!(builder.finish(), None);
    }

    #[test]
    fn song_builder_deprecated_time_field() {
        let mut builder = SongBuilder::default();

        assert_matches!(builder.field("file", String::from("foo.flac")), Ok(None));

        assert_matches!(builder.field("Time", String::from("123")), Ok(None));
        assert_eq!(builder.duration, Some(Duration::from_secs(123)));

        assert_matches!(builder.field("duration", String::from("456.700")), Ok(None));
        assert_eq!(builder.duration, Some(Duration::from_secs_f64(456.7)));

        assert_matches!(builder.field("Time", String::from("123")), Ok(None));
        assert_eq!(builder.duration, Some(Duration::from_secs_f64(456.7)));

        let song = builder.finish().unwrap().song;

        assert_eq!(
            song,
            Song {
                url: String::from("foo.flac"),
                format: None,
                last_modified: None,
                duration: Some(Duration::from_secs_f64(456.7)),
                tags: HashMap::new(),
            }
        );
    }

    #[test]
    fn parse_range() {
        assert_eq!(
            SongRange::from_value(String::from("1.500-5.642"), "Range").unwrap(),
            SongRange {
                from: Duration::from_secs_f64(1.5),
                to: Some(Duration::from_secs_f64(5.642)),
            }
        );

        assert_eq!(
            SongRange::from_value(String::from("1.500-"), "Range").unwrap(),
            SongRange {
                from: Duration::from_secs_f64(1.5),
                to: None,
            }
        );

        assert_matches!(SongRange::from_value(String::from("foo"), "Range"), Err(_));

        assert_matches!(
            SongRange::from_value(String::from("1.000--5.000"), "Range"),
            Err(_)
        );
    }
}
