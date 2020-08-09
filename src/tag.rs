//! Metadata tags.

use std::borrow::Cow;
use std::convert::TryFrom;
use std::error::Error;
use std::fmt;

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
/// [`Song`]: crate::commands::responses::Song
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
#[non_exhaustive]
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
            "AlbumSort" => Self::AlbumSort,
            "AlbumArtist" => Self::AlbumArtist,
            "AlbumArtistSort" => Self::AlbumArtistSort,
            "Artist" => Self::Artist,
            "ArtistSort" => Self::ArtistSort,
            "Comment" => Self::Comment,
            "Composer" => Self::Composer,
            "Date" => Self::Date,
            "OriginalDate" => Self::OriginalDate,
            "Disc" => Self::Disc,
            "Genre" => Self::Genre,
            "Label" => Self::Label,
            "MUSICBRAINZ_ARTISTID" => Self::MusicBrainzArtistId,
            "MUSICBRAINZ_TRACKID" => Self::MusicBrainzRecordingId,
            "MUSICBRAINZ_ALBUMARTISTID" => Self::MusicBrainzReleaseArtistId,
            "MUSICBRAINZ_ALBUMID" => Self::MusicBrainzReleaseId,
            "MUSICBRAINZ_RELEASETRACKID" => Self::MusicBrainzTrackId,
            "MUSICBRAINZ_WORKID" => Self::MusicBrainzWorkId,
            "Name" => Self::Name,
            "Performer" => Self::Performer,
            "Title" => Self::Title,
            "Track" => Self::Track
        }

        Ok(Self::Other(raw.into()))
    }
}

impl Argument for Tag {
    fn render(self) -> Cow<'static, str> {
        self.as_str()
    }
}

/// Errors that may occur when attempting to create a [`Tag`].
///
/// [`Tag`]: crate::tag::Tag
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
            Self::Empty => write!(f, "Empty tag"),
            Self::InvalidCharacter { chr, pos } => {
                write!(f, "Invalid character {:?} at index {}", chr, pos)
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
}
