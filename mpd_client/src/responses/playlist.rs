use mpd_protocol::response::Frame;

use crate::responses::{FromFieldValue, Timestamp, TypedResponseError};

/// A stored playlist, as returned by [`listplaylists`].
///
/// [`listplaylists`]: crate::commands::definitions::GetPlaylists
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Playlist {
    /// Name of the playlist.
    pub name: String,
    /// Server timestamp of last modification.
    pub last_modified: Timestamp,
}

impl Playlist {
    pub(crate) fn parse_frame(frame: Frame) -> Result<Vec<Self>, TypedResponseError> {
        let mut out = Vec::with_capacity(frame.fields_len() / 2);

        let mut current_name: Option<String> = None;

        for (key, value) in frame {
            if let Some(name) = current_name.take() {
                if key.as_ref() == "Last-Modified" {
                    let last_modified = Timestamp::from_value(value, "Last-Modified")?;

                    out.push(Playlist {
                        name,
                        last_modified,
                    });
                } else {
                    return Err(TypedResponseError::unexpected_field(
                        "Last-Modified",
                        key.as_ref(),
                    ));
                }
            } else if key.as_ref() == "playlist" {
                current_name = Some(value);
            } else {
                return Err(TypedResponseError::unexpected_field(
                    "playlist",
                    key.as_ref(),
                ));
            }
        }

        Ok(out)
    }
}
