use mpd_protocol::response::Frame;

use std::collections::HashMap;
use std::iter;
use std::num::ParseIntError;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use super::{ErrorKind, SongIdentifier, TypedResponseError};

/// A single song, as returned by the playlist or current song commands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Song {
    /// Raw `file` key as returned by MPD. This may be a file path relative to the library root, or
    /// a full URL to some remote resource.
    pub file: String,
    /// The `duration` as returned by MPD.
    pub duration: Option<Duration>,
    /// Identifiers of this song. Depending on the context of the command, this may be `None`.
    pub identifiers: Option<SongIdentifier>,
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
        .collect()
    }

    fn new(file: String) -> Self {
        Self {
            file,
            duration: None,
            identifiers: None,
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
    type Item = Result<Song, TypedResponseError>;

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
                    Ok(v) => song_pos = Some(v),
                    Err(e) => return Some(Err(parse_field_error("Pos", e))),
                },
                "Id" => match value.parse() {
                    Ok(v) => song_id = Some(v),
                    Err(e) => return Some(Err(parse_field_error("Id", e))),
                },
                _ => {
                    let tag = Tag::from_str(key);
                    song.tags.entry(tag).or_default().push(value);
                }
            }
        }

        if let (Some(pos), Some(id)) = (song_pos, song_id) {
            song.identifiers = Some(SongIdentifier { pos, id });
        }

        Some(Ok(song))
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
}
