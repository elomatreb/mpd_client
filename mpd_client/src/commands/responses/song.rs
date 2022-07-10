use chrono::{DateTime, FixedOffset};

use std::cmp;
use std::collections::HashMap;
use std::iter;
use std::num::ParseIntError;
use std::path::Path;
use std::time::Duration;

use super::{parse_duration, KeyValuePair};
use crate::commands::{SongId, SongPosition};
use crate::errors::{ErrorKind, TypedResponseError};
use crate::tag::Tag;

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

/// Range used when playing only part of a [`Song`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SongRange {
    /// Start playback at this timestamp.
    pub from: Duration,
    /// End at this timestamp (if the end is known).
    pub to: Option<Duration>,
}

impl SongInQueue {
    pub(crate) fn parse_frame(
        frame: impl IntoIterator<Item = KeyValuePair>,
        max_count: Option<usize>,
    ) -> Result<Vec<Self>, TypedResponseError> {
        let max_count = max_count.unwrap_or(usize::max_value());
        assert!(max_count > 0);

        let mut fields = frame.into_iter().peekable();

        SongIter {
            fields: &mut fields,
        }
        .take(max_count)
        .map(|res| {
            res.and_then(|(song, identifier)| match identifier {
                Some(SongQueueData {
                    position,
                    id,
                    range,
                    priority,
                }) => Ok(SongInQueue {
                    position,
                    id,
                    range,
                    priority,
                    song,
                }),
                None => Err(TypedResponseError {
                    field: "Id",
                    kind: ErrorKind::Missing,
                }),
            })
        })
        .collect()
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
    pub last_modified: Option<DateTime<FixedOffset>>,
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

    /// Get the track and disc number of the song.
    ///
    /// If either are not set on the song, 0 is returned. This is a utility for sorting.
    pub fn number(&self) -> (u64, u64) {
        let track = parse_number(self.single_tag_value(&Tag::Track));
        let disc = parse_number(self.single_tag_value(&Tag::Disc));

        (track, disc)
    }

    pub(crate) fn parse_frame(
        frame: impl IntoIterator<Item = KeyValuePair>,
        max_count: Option<usize>,
    ) -> Result<Vec<Self>, TypedResponseError> {
        let max_count = max_count.unwrap_or(usize::max_value());
        assert!(max_count > 0);

        let mut fields = frame.into_iter().peekable();

        SongIter {
            fields: &mut fields,
        }
        .take(max_count)
        .map(|r| r.map(|(song, _)| song))
        .collect()
    }

    fn new(url: String) -> Self {
        Self {
            url,
            duration: None,
            format: None,
            tags: HashMap::new(),
            last_modified: None,
        }
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

fn parse_number(val: Option<&str>) -> u64 {
    match val {
        None => 0,
        Some(v) => v.parse().unwrap_or(0),
    }
}

struct SongIter<'a, I: Iterator> {
    fields: &'a mut iter::Peekable<I>,
}

/// Utility struct that holds the intermediate results for a [`SongInQueue`].
struct SongQueueData {
    position: SongPosition,
    id: SongId,
    range: Option<SongRange>,
    priority: u8,
}

impl<'a, I> Iterator for SongIter<'a, I>
where
    I: Iterator<Item = KeyValuePair>,
{
    type Item = Result<(Song, Option<SongQueueData>), TypedResponseError>;

    fn next(&mut self) -> Option<Self::Item> {
        let (key, value) = self
            .fields
            // Skip directory or playlist entries, encountered when using e.g. the `listallinfo`
            // command, as well as the associated modification time.
            .find(|(k, _)| !matches!(&**k, "directory" | "playlist" | "Last-Modified"))?;

        let mut song = if &*key == "file" {
            Song::new(value)
        } else {
            return Some(Err(TypedResponseError {
                field: "file",
                kind: ErrorKind::UnexpectedField(key.as_ref().to_owned()),
            }));
        };

        let mut song_pos = None;
        let mut song_id = None;
        let mut range = None;
        let mut priority = 0;

        let mut deprecated_time = None;

        loop {
            match self.fields.peek() {
                Some((k, _)) => {
                    // If the next key starts another file, the current iteration is done
                    if let "file" | "directory" | "playlist" = &**k {
                        break;
                    }
                }
                None => break,
            }

            let (key, value) = self.fields.next().unwrap();
            match &*key {
                "file" | "directory" | "playlist" => unreachable!(),
                "duration" => match parse_duration("duration", &value) {
                    Ok(d) => song.duration = Some(d),
                    Err(e) => return Some(Err(e)),
                },
                // Just a worse "duration" field, but present for backwards compatibility with
                // protocol versions <0.20
                "Time" => deprecated_time = Some(value),
                "Range" => match parse_range_field(value) {
                    Ok(r) => range = Some(r),
                    Err(e) => return Some(Err(e)),
                },
                "Format" => song.format = Some(value),
                "Last-Modified" => {
                    let ts = match DateTime::parse_from_rfc3339(&value) {
                        Ok(ts) => ts,
                        Err(e) => {
                            return Some(Err(TypedResponseError {
                                field: "Last-Modified",
                                kind: ErrorKind::MalformedTimestamp(e),
                            }))
                        }
                    };

                    song.last_modified = Some(ts);
                }
                "Prio" => match value.parse() {
                    Ok(v) => priority = v,
                    Err(e) => return Some(Err(parse_field_error("Prio", e))),
                },
                "Pos" => match value.parse() {
                    Ok(v) => song_pos = Some(SongPosition(v)),
                    Err(e) => return Some(Err(parse_field_error("Pos", e))),
                },
                "Id" => match value.parse() {
                    Ok(v) => song_id = Some(SongId(v)),
                    Err(e) => return Some(Err(parse_field_error("Id", e))),
                },
                _ => {
                    if let Ok(tag) = Tag::try_from(&*key) {
                        song.tags.entry(tag).or_default().push(value);
                    }
                }
            }
        }

        let range = range.map(|(from, to)| {
            // Clamp range to end of song if known
            let to = cmp::max(to, song.duration);

            SongRange { from, to }
        });

        // Fall back to deprecated `Time` field for the duration if we found no `duration` field.
        if song.duration.is_none() && deprecated_time.is_some() {
            match parse_duration("Time", deprecated_time.as_deref().unwrap()) {
                Ok(d) => song.duration = Some(d),
                Err(e) => return Some(Err(e)),
            }
        }

        let queue_data = match (song_pos, song_id) {
            (Some(position), Some(id)) => Some(SongQueueData {
                position,
                id,
                range,
                priority,
            }),
            _ => None,
        };

        Some(Ok((song, queue_data)))
    }
}

fn parse_range_field(raw: String) -> Result<(Duration, Option<Duration>), TypedResponseError> {
    // The range follows the form "<start>-<end?>"
    let (from, to) = match raw.split_once('-') {
        Some(v) => v,
        None => {
            return Err(TypedResponseError {
                field: "Range",
                kind: ErrorKind::InvalidValue(raw),
            });
        }
    };

    let from = parse_duration("Range", from)?;

    let to = if to.is_empty() {
        None
    } else {
        Some(parse_duration("Range", to)?)
    };

    Ok((from, to))
}

fn parse_field_error(field: &'static str, error: ParseIntError) -> TypedResponseError {
    TypedResponseError {
        field,
        kind: ErrorKind::MalformedInteger(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn key_value_pairs(
        raw: Vec<(&'static str, &'static str)>,
    ) -> impl Iterator<Item = KeyValuePair> {
        raw.into_iter().map(|(k, v)| (Arc::from(k), v.to_owned()))
    }

    #[test]
    fn song_parser() {
        let ts = "2020-06-12T17:53:00Z";
        let input = key_value_pairs(vec![
            ("file", "test.flac"),
            ("duration", "123.456"),
            ("Last-Modified", ts),
            ("Artist", "Foo"),
            ("Artist", "Bar"),
            ("UnknownTag", "spooky value"),
            ("directory", "foo"),
            ("file", "foo/bar.baz"),
            ("directory", "this"),
            ("directory", "this/one"),
            ("directory", "this/one/should"),
            ("file", "this/one/should/be.ignored"),
            ("directory", "empty_dir"),
        ]);

        let songs = Song::parse_frame(input, Some(2)).unwrap();

        assert_eq!(songs.len(), 2);

        assert_eq!(songs[0].url, "test.flac");
        assert_eq!(songs[0].duration, Some(Duration::from_secs_f64(123.456)));
        assert_eq!(songs[0].format, None);
        assert_eq!(
            songs[0].last_modified,
            Some(DateTime::parse_from_rfc3339(ts).unwrap())
        );
        assert_eq!(songs[0].artists(), &["Foo", "Bar"]);
        assert_eq!(songs[0].title(), None);
        assert_eq!(
            songs[0].tags.get(&Tag::Other("UnknownTag".into())),
            Some(&vec![String::from("spooky value")])
        );

        assert_eq!(songs[1].url, "foo/bar.baz");
        assert_eq!(songs[1].tags.len(), 0);
        assert_eq!(songs[1].duration, None);
        assert_eq!(songs[1].last_modified, None);
        assert_eq!(songs[1].format, None);
    }

    #[test]
    fn song_parser_directory_with_modified() {
        // https://github.com/elomatreb/mpd_client/issues/7
        let input = key_value_pairs(vec![
            ("directory", "foo"),
            ("Last-Modified", "2020-06-12T17:53:00Z"),
            ("file", "foo/bar.baz"),
        ]);

        let songs = Song::parse_frame(input, None).unwrap();

        assert_eq!(
            songs,
            vec![Song {
                url: String::from("foo/bar.baz"),
                duration: None,
                tags: HashMap::new(),
                format: None,
                last_modified: None,
            }]
        );
    }

    #[test]
    fn song_parser_playlist_item() {
        // https://github.com/elomatreb/mpd_client/issues/11
        let input = key_value_pairs(vec![
            ("file", "foo/bar.baz"),
            ("playlist", "foo/asdf.m3u"),
            ("file", "foo/qux.baz"),
        ]);

        let songs = Song::parse_frame(input, None).unwrap();

        assert_eq!(
            songs,
            vec![
                Song {
                    url: String::from("foo/bar.baz"),
                    duration: None,
                    tags: HashMap::new(),
                    format: None,
                    last_modified: None,
                },
                Song {
                    url: String::from("foo/qux.baz"),
                    duration: None,
                    tags: HashMap::new(),
                    format: None,
                    last_modified: None,
                }
            ]
        );
    }

    #[test]
    fn song_parser_invalid_durations() {
        let input = key_value_pairs(vec![("file", "foo/bar.baz"), ("duration", "-1")]);
        let err = Song::parse_frame(input, None).unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidTimestamp);
        assert_eq!(err.field, "duration");

        let input = key_value_pairs(vec![("file", "foo/bar.baz"), ("Time", "-1")]);
        let err = Song::parse_frame(input, None).unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidTimestamp);
        assert_eq!(err.field, "Time");

        let input = key_value_pairs(vec![("file", "foo/bar.baz"), ("duration", "NaN")]);
        let err = Song::parse_frame(input, None).unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidTimestamp);
        assert_eq!(err.field, "duration");

        let max = Box::leak(f64::MAX.to_string().into_boxed_str());
        let input = key_value_pairs(vec![("file", "foo/bar.baz"), ("duration", &*max)]);
        let err = Song::parse_frame(input, None).unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidTimestamp);
        assert_eq!(err.field, "duration");
    }

    #[test]
    fn song_parser_deprecated_time_field() {
        let input = key_value_pairs(vec![("file", "foo/bar.baz"), ("Time", "123")]);

        let songs = Song::parse_frame(input, None).unwrap();

        assert_eq!(
            songs,
            vec![Song {
                url: String::from("foo/bar.baz"),
                duration: Some(Duration::from_secs(123)),
                tags: HashMap::new(),
                format: None,
                last_modified: None,
            }]
        );

        // Duration field takes priority:
        let input = key_value_pairs(vec![
            ("file", "foo/bar.baz"),
            ("duration", "456.789"),
            ("Time", "123"),
        ]);

        let songs = Song::parse_frame(input, None).unwrap();

        assert_eq!(
            songs,
            vec![Song {
                url: String::from("foo/bar.baz"),
                duration: Some(Duration::from_secs_f64(456.789)),
                tags: HashMap::new(),
                format: None,
                last_modified: None,
            }]
        );
    }

    #[test]
    fn song_in_queue_parser() {
        let ts = "2020-06-12T17:53:00Z";
        let input = key_value_pairs(vec![
            ("file", "test.flac"),
            ("duration", "123.456"),
            ("Last-Modified", ts),
            ("Artist", "Foo"),
            ("Artist", "Bar"),
            ("UnknownTag", "spooky value"),
            ("Pos", "2"),
            ("Id", "1234"),
            ("directory", "foo"),
            ("file", "foo/bar.baz"),
            ("Prio", "101"),
            ("Pos", "3"),
            ("Id", "1337"),
        ]);

        let songs = SongInQueue::parse_frame(input, None).unwrap();

        assert_eq!(songs.len(), 2);

        assert_eq!(songs[0].id, 1234.into());
        assert_eq!(songs[0].position, 2.into());
        assert_eq!(songs[0].priority, 0);
        assert_eq!(songs[0].song.url, "test.flac");
        assert_eq!(
            songs[0].song.duration,
            Some(Duration::from_secs_f64(123.456))
        );
        assert_eq!(songs[0].song.format, None);
        assert_eq!(
            songs[0].song.last_modified,
            Some(DateTime::parse_from_rfc3339(ts).unwrap())
        );
        assert_eq!(songs[0].song.artists(), &["Foo", "Bar"]);
        assert_eq!(songs[0].song.title(), None);
        assert_eq!(
            songs[0].song.tags.get(&Tag::Other("UnknownTag".into())),
            Some(&vec![String::from("spooky value")])
        );

        assert_eq!(songs[1].id, 1337.into());
        assert_eq!(songs[1].position, 3.into());
        assert_eq!(songs[1].priority, 101);
        assert_eq!(songs[1].song.url, "foo/bar.baz");
        assert_eq!(songs[1].song.tags.len(), 0);
        assert_eq!(songs[1].song.duration, None);
        assert_eq!(songs[1].song.last_modified, None);
        assert_eq!(songs[1].song.format, None);
    }

    #[test]
    fn parse_range() {
        assert_eq!(
            parse_range_field(String::from("1.500-5.642")),
            Ok((
                Duration::from_secs_f64(1.5),
                Some(Duration::from_secs_f64(5.642))
            ))
        );

        assert_eq!(
            parse_range_field(String::from("1.500-")),
            Ok((Duration::from_secs_f64(1.5), None))
        );

        assert_eq!(
            parse_range_field(String::from("foo")),
            Err(TypedResponseError {
                field: "Range",
                kind: ErrorKind::InvalidValue(String::from("foo")),
            })
        );

        assert_eq!(
            parse_range_field(String::from("1.000--5.000")),
            Err(TypedResponseError {
                field: "Range",
                kind: ErrorKind::InvalidTimestamp,
            }),
        );
    }
}
