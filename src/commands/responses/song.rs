use mpd_protocol::response::Frame;

use std::borrow::Cow;
use std::collections::HashMap;
use std::iter;
use std::num::ParseIntError;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use super::{ErrorKind, TypedResponseError};
use crate::commands::{SongId, SongPosition};

/// A [`Song`] in the current queue.
///
/// [`Song`]: struct.Song.html
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SongInQueue {
    /// Position in queue.
    pub position: SongPosition,
    /// ID in queue.
    pub id: SongId,
    /// The song.
    pub song: Song,
}

impl SongInQueue {
    pub(super) fn parse_frame(
        frame: Frame,
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
                Some((position, id)) => Ok(SongInQueue { position, id, song }),
                None => Err(TypedResponseError {
                    field: "Id",
                    kind: ErrorKind::Missing,
                }),
            })
        })
        .collect()
    }
}

/// A single song, as returned by the playlist or current song commands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Song {
    /// Raw `file` key as returned by MPD. This may be a file path relative to the library root, or
    /// a full URL to some remote resource.
    pub file: String,
    /// The `duration` as returned by MPD.
    pub duration: Option<Duration>,
    /// Tags in this response.
    pub tags: HashMap<Tag, Vec<String>>,
}

impl Song {
    /// Get the file as a `Path`. Note that if the file is a remote URL, operations on the result
    /// will give unexpected results.
    pub fn file_path(&self) -> &Path {
        Path::new(&self.file)
    }

    pub(super) fn parse_frame(
        frame: Frame,
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

    fn new(file: String) -> Self {
        Self {
            file,
            duration: None,
            tags: HashMap::new(),
        }
    }
}

struct SongIter<'a, I: Iterator> {
    fields: &'a mut iter::Peekable<I>,
}

impl<'a, I> Iterator for SongIter<'a, I>
where
    I: Iterator<Item = (Arc<str>, String)>,
{
    type Item = Result<(Song, Option<(SongPosition, SongId)>), TypedResponseError>;

    fn next(&mut self) -> Option<Self::Item> {
        let (key, value) = self.fields.next()?;

        let mut song = if key.as_ref() == "file" {
            Song::new(value)
        } else {
            return Some(Err(TypedResponseError {
                field: "file",
                kind: ErrorKind::UnexpectedField(key.as_ref().to_owned()),
            }));
        };

        let mut song_pos = None;
        let mut song_id = None;

        loop {
            match self.fields.peek() {
                Some((k, _)) => {
                    // If the next key starts another file, the current iteration is done
                    if k.as_ref() == "file" {
                        break;
                    }
                }
                None => break,
            }

            let (key, value) = self.fields.next().unwrap();
            match key.as_ref() {
                "file" => unreachable!(),
                "duration" => match value.parse() {
                    Ok(v) => song.duration = Some(Duration::from_secs_f64(v)),
                    Err(e) => {
                        return Some(Err(TypedResponseError {
                            field: "duration",
                            kind: ErrorKind::MalformedFloat(e),
                        }))
                    }
                },
                // Ignored keys for now
                "Last-Modified" | "Time" | "Range" | "Format" => (),
                "Pos" => match value.parse() {
                    Ok(v) => song_pos = Some(SongPosition(v)),
                    Err(e) => return Some(Err(parse_field_error("Pos", e))),
                },
                "Id" => match value.parse() {
                    Ok(v) => song_id = Some(SongId(v)),
                    Err(e) => return Some(Err(parse_field_error("Id", e))),
                },
                _ => {
                    let tag = Tag::from_str(key);
                    song.tags.entry(tag).or_default().push(value);
                }
            }
        }

        let identifier = match (song_pos, song_id) {
            (Some(pos), Some(id)) => Some((pos, id)),
            _ => None,
        };

        Some(Ok((song, identifier)))
    }
}

fn parse_field_error(field: &'static str, error: ParseIntError) -> TypedResponseError {
    TypedResponseError {
        field,
        kind: ErrorKind::MalformedInteger(error),
    }
}

/// Tags which can be set on a [`Song`].
///
/// MusicBrainz tags are named differently from how they appear in the protocol to better reflect
/// their actual purpose.
///
/// [`Song`]: struct.Song.html
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum Tag {
    Album,
    AlbumSort,
    AlbumArtist,
    AlbumArtistSort,
    Artist,
    ArtistSort,
    Comment,
    Composer,
    Date,
    OriginalDate,
    Disc,
    Genre,
    Label,
    MusicBrainzArtistId,
    MusicBrainzRecordingId,
    MusicBrainzReleaseArtistId,
    MusicBrainzReleaseId,
    MusicBrainzTrackId,
    MusicBrainzWorkId,
    Name,
    Performer,
    Title,
    Track,
    Other(Arc<str>),
}

impl Tag {
    fn from_str(raw: Arc<str>) -> Self {
        use Tag::*;
        match raw.as_ref() {
            "Album" => Album,
            "AlbumSort" => AlbumSort,
            "AlbumArtist" => AlbumArtist,
            "AlbumArtistSort" => AlbumArtistSort,
            "Artist" => Artist,
            "ArtistSort" => ArtistSort,
            "Comment" => Comment,
            "Composer" => Composer,
            "Date" => Date,
            "OriginalDate" => OriginalDate,
            "Disc" => Disc,
            "Genre" => Genre,
            "Label" => Label,
            "MUSICBRAINZ_ARTISTID" => MusicBrainzArtistId,
            "MUSICBRAINZ_TRACKID" => MusicBrainzRecordingId,
            "MUSICBRAINZ_ALBUMARTISTID" => MusicBrainzReleaseArtistId,
            "MUSICBRAINZ_ALBUMID" => MusicBrainzReleaseId,
            "MUSICBRAINZ_RELEASETRACKID" => MusicBrainzTrackId,
            "MUSICBRAINZ_WORKID" => MusicBrainzWorkId,
            "Name" => Name,
            "Performer" => Performer,
            "Title" => Title,
            "Track" => Track,
            _ => Other(raw),
        }
    }

    pub(crate) fn as_argument(&self) -> Cow<'static, str> {
        if let Tag::Other(raw) = self {
            return Cow::Owned(raw.to_string());
        }

        Cow::Borrowed(match self {
            Tag::Other(_) => unreachable!(),
            Tag::Album => "Album",
            Tag::AlbumSort => "AlbumSort",
            Tag::AlbumArtist => "AlbumArtist",
            Tag::AlbumArtistSort => "AlbumArtistSort",
            Tag::Artist => "Artist",
            Tag::ArtistSort => "ArtistSort",
            Tag::Comment => "Comment",
            Tag::Composer => "Composer",
            Tag::Date => "Date",
            Tag::OriginalDate => "OriginalDate",
            Tag::Disc => "Disc",
            Tag::Genre => "Genre",
            Tag::Label => "Label",
            Tag::MusicBrainzArtistId => "MUSICBRAINZ_ARTISTID",
            Tag::MusicBrainzRecordingId => "MUSICBRAINZ_TRACKID",
            Tag::MusicBrainzReleaseArtistId => "MUSICBRAINZ_ALBUMARTISTID",
            Tag::MusicBrainzReleaseId => "MUSICBRAINZ_ALBUMID",
            Tag::MusicBrainzTrackId => "MUSICBRAINZ_RELEASETRACKID",
            Tag::MusicBrainzWorkId => "MUSICBRAINZ_WORKID",
            Tag::Name => "Name",
            Tag::Performer => "Performer",
            Tag::Title => "Title",
            Tag::Track => "Track",
        })
    }
}
