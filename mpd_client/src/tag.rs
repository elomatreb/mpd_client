//! Metadata tags.

use std::borrow::Cow;
use std::error::Error;
use std::fmt;
use std::hash::{Hash, Hasher};

use bytes::{BufMut, BytesMut};
use mpd_protocol::command::Argument;

/// Tags which can be set on a [`Song`].
///
/// MusicBrainz tags are named differently from how they appear in the protocol to better reflect
/// their actual purpose.
///
/// # Tag validity
///
/// **Manually** constructing a tag with the `Other` variant may result in protocols errors if the
/// tag is invalid. Use the `TryFrom` implementation for checked conversion.
///
/// # Unknown tags
///
/// When parsing or constructing responses, tags not recognized by this type will be stored as they
/// are encountered using the `Other` variant. Additionally the enum is marked as non-exhaustive,
/// so additional tags may be added without breaking compatibility.
///
/// The equality is checked using the string representation, so `Other` variants are
/// forward-compatible with new variants being added.
///
/// [`Song`]: crate::commands::responses::Song
#[derive(Clone, Debug)]
#[allow(missing_docs)]
#[non_exhaustive]
pub enum Tag {
    Album,
    AlbumArtist,
    AlbumArtistSort,
    AlbumSort,
    Artist,
    ArtistSort,
    Comment,
    Composer,
    ComposerSort,
    Conductor,
    Date,
    Disc,
    Ensemble,
    Genre,
    Grouping,
    Label,
    Location,
    Movement,
    MovementNumber,
    MusicBrainzArtistId,
    MusicBrainzRecordingId,
    MusicBrainzReleaseArtistId,
    MusicBrainzReleaseId,
    MusicBrainzTrackId,
    MusicBrainzWorkId,
    Name,
    OriginalDate,
    Performer,
    Title,
    Track,
    Work,
    /// Catch-all variant that contains the raw tag string when it doesn't match any other
    /// variants, but is valid.
    Other(Box<str>),
}

impl Tag {
    /// Creates a tag for [filtering] which will match *any* tag.
    ///
    /// [filtering]: crate::filter::Filter
    pub fn any() -> Self {
        Self::Other("any".into())
    }

    pub(crate) fn as_str(&self) -> Cow<'static, str> {
        Cow::Borrowed(match self {
            Tag::Other(raw) => return Cow::Owned(raw.to_string()),
            Tag::Album => "Album",
            Tag::AlbumArtist => "AlbumArtist",
            Tag::AlbumArtistSort => "AlbumArtistSort",
            Tag::AlbumSort => "AlbumSort",
            Tag::Artist => "Artist",
            Tag::ArtistSort => "ArtistSort",
            Tag::Comment => "Comment",
            Tag::Composer => "Composer",
            Tag::ComposerSort => "ComposerSort",
            Tag::Conductor => "Conductor",
            Tag::Date => "Date",
            Tag::Disc => "Disc",
            Tag::Ensemble => "Ensemble",
            Tag::Genre => "Genre",
            Tag::Grouping => "Grouping",
            Tag::Label => "Label",
            Tag::Location => "Location",
            Tag::Movement => "Movement",
            Tag::MovementNumber => "MovementNumber",
            Tag::MusicBrainzArtistId => "MUSICBRAINZ_ARTISTID",
            Tag::MusicBrainzRecordingId => "MUSICBRAINZ_TRACKID",
            Tag::MusicBrainzReleaseArtistId => "MUSICBRAINZ_ALBUMARTISTID",
            Tag::MusicBrainzReleaseId => "MUSICBRAINZ_ALBUMID",
            Tag::MusicBrainzTrackId => "MUSICBRAINZ_RELEASETRACKID",
            Tag::MusicBrainzWorkId => "MUSICBRAINZ_WORKID",
            Tag::Name => "Name",
            Tag::OriginalDate => "OriginalDate",
            Tag::Performer => "Performer",
            Tag::Title => "Title",
            Tag::Track => "Track",
            Tag::Work => "Work",
        })
    }
}

macro_rules! match_ignore_case {
    ($raw:ident, $($pattern:literal => $result:expr),+) => {
        $(
            if $raw.eq_ignore_ascii_case($pattern) {
                return Ok($result);
            }
        )+
    };
}

impl<'a> TryFrom<&'a str> for Tag {
    type Error = TagError;

    fn try_from(raw: &'a str) -> Result<Self, Self::Error> {
        if raw.is_empty() {
            return Err(TagError::Empty);
        } else if let Some((pos, chr)) = raw
            .char_indices()
            .find(|&(_, ch)| !(ch.is_ascii_alphabetic() || ch == '_' || ch == '-'))
        {
            return Err(TagError::InvalidCharacter { chr, pos });
        }

        match_ignore_case! {
            raw,
            "Album" => Self::Album,
            "AlbumArtist" => Self::AlbumArtist,
            "AlbumArtistSort" => Self::AlbumArtistSort,
            "AlbumSort" => Self::AlbumSort,
            "Artist" => Self::Artist,
            "ArtistSort" => Self::ArtistSort,
            "Comment" => Self::Comment,
            "Composer" => Self::Composer,
            "ComposerSort" => Self::ComposerSort,
            "Conductor" => Self::Conductor,
            "Date" => Self::Date,
            "Disc" => Self::Disc,
            "Ensemble" => Self::Ensemble,
            "Genre" => Self::Genre,
            "Grouping" => Self::Grouping,
            "Label" => Self::Label,
            "Location" => Self::Location,
            "Movement" => Self::Movement,
            "MovementNumber" => Self::MovementNumber,
            "MUSICBRAINZ_ALBUMARTISTID" => Self::MusicBrainzReleaseArtistId,
            "MUSICBRAINZ_ALBUMID" => Self::MusicBrainzReleaseId,
            "MUSICBRAINZ_ARTISTID" => Self::MusicBrainzArtistId,
            "MUSICBRAINZ_RELEASETRACKID" => Self::MusicBrainzTrackId,
            "MUSICBRAINZ_TRACKID" => Self::MusicBrainzRecordingId,
            "MUSICBRAINZ_WORKID" => Self::MusicBrainzWorkId,
            "Name" => Self::Name,
            "OriginalDate" => Self::OriginalDate,
            "Performer" => Self::Performer,
            "Title" => Self::Title,
            "Track" => Self::Track,
            "Work" => Self::Work
        }

        Ok(Self::Other(raw.into()))
    }
}

impl PartialEq for Tag {
    fn eq(&self, other: &Tag) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Eq for Tag {}

impl<'a> PartialEq<&'a str> for Tag {
    fn eq(&self, other: &&'a str) -> bool {
        self.as_str() == *other
    }
}

impl PartialOrd for Tag {
    fn partial_cmp(&self, other: &Tag) -> Option<std::cmp::Ordering> {
        self.as_str().partial_cmp(&other.as_str())
    }
}

impl Ord for Tag {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_str().cmp(&other.as_str())
    }
}

impl Hash for Tag {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }
}

impl Argument for Tag {
    fn render(&self, buf: &mut BytesMut) {
        buf.put_slice(self.as_str().as_bytes());
    }
}

/// Errors that may occur when attempting to create a [`Tag`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TagError {
    /// The raw tag was empty.
    Empty,
    /// The raw tag contained an invalid character.
    InvalidCharacter {
        /// The character.
        chr: char,
        /// Byte position of `chr`.
        pos: usize,
    },
}

impl fmt::Display for TagError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "empty tag"),
            Self::InvalidCharacter { chr, pos } => {
                write!(f, "invalid character {:?} at index {}", chr, pos)
            }
        }
    }
}

impl Error for TagError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_from() {
        assert_eq!(Tag::try_from("Artist"), Ok(Tag::Artist));

        // case-insensitive
        assert_eq!(Tag::try_from("artist"), Ok(Tag::Artist));

        // unrecognized but valid tag
        assert_eq!(Tag::try_from("foo"), Ok(Tag::Other(Box::from("foo"))));
    }

    #[test]
    fn try_from_error() {
        assert_eq!(Tag::try_from(""), Err(TagError::Empty));
        assert_eq!(
            Tag::try_from("foo bar"),
            Err(TagError::InvalidCharacter { chr: ' ', pos: 3 })
        );
    }

    #[test]
    fn as_arg() {
        assert_eq!(Tag::Album.as_str(), "Album");
        assert_eq!(Tag::Other(Box::from("foo")).as_str(), "foo");
    }

    #[test]
    fn equality() {
        assert_eq!(Tag::Album, Tag::Other(Box::from("Album")));
        assert_eq!(
            Tag::Other(Box::from("Album")),
            Tag::Other(Box::from("Album"))
        );
        assert_ne!(Tag::Other(Box::from("Foo")), Tag::Other(Box::from("Bar")));

        assert_eq!(Tag::Artist, "Artist");
        assert_eq!(Tag::Other(Box::from("Foo")), "Foo");
    }
}
