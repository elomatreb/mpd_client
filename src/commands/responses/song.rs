use mpd_protocol::response::Frame;

use std::collections::HashMap;
use std::convert::TryFrom;
use std::iter;
use std::num::ParseIntError;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use super::{ErrorKind, TypedResponseError};
use crate::commands::{SongId, SongPosition};
use crate::tag::Tag;

/// A [`Song`] in the current queue.
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
    /// Unique identifier of the song. May be a file path relative to the library root, or an URL
    /// to a remote resource.
    ///
    /// This is the `file` key as returned by MPD.
    pub url: String,
    /// The `duration` as returned by MPD.
    pub duration: Option<Duration>,
    /// Tags in this response.
    pub tags: HashMap<Tag, Vec<String>>,
}

impl Song {
    /// Get the file as a `Path`. Note that if the file is a remote URL, operations on the result
    /// will give unexpected results.
    pub fn file_path(&self) -> &Path {
        Path::new(&self.url)
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

    fn new(url: String) -> Self {
        Self {
            url,
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
                    if let Ok(tag) = Tag::try_from(&*key) {
                        song.tags.entry(tag).or_default().push(value);
                    }
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
