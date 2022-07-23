use std::sync::Arc;

use chrono::{DateTime, FixedOffset};

use crate::commands::TypedResponseError;

/// A stored playlist, as returned by [`listplaylists`].
///
/// [`listplaylists`]: crate::commands::definitions::GetPlaylists
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Playlist {
    /// Name of the playlist.
    pub name: String,
    /// Server timestamp of last modification.
    pub last_modified: DateTime<FixedOffset>,
}

impl Playlist {
    pub(crate) fn parse_frame(
        frame: impl IntoIterator<Item = (Arc<str>, String)>,
        field_count: usize,
    ) -> Result<Vec<Self>, TypedResponseError> {
        let fields = frame.into_iter();
        let mut out = Vec::with_capacity(field_count / 2);

        let mut current_name: Option<String> = None;

        for (key, value) in fields {
            if let Some(name) = current_name.take() {
                if key.as_ref() == "Last-Modified" {
                    let last_modified = DateTime::parse_from_rfc3339(&value).map_err(|e| {
                        TypedResponseError::invalid_value("Last-Modified".into(), value).source(e)
                    })?;

                    out.push(Playlist {
                        name,
                        last_modified,
                    });
                } else {
                    return Err(TypedResponseError::unexpected_field(
                        String::from("Last-Modified"),
                        key.as_ref().into(),
                    ));
                }
            } else if key.as_ref() == "playlist" {
                current_name = Some(value);
            } else {
                return Err(TypedResponseError::unexpected_field(
                    String::from("playlist"),
                    key.as_ref().into(),
                ));
            }
        }

        Ok(out)
    }
}
