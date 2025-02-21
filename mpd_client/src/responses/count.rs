use std::time::Duration;

use mpd_protocol::response::Frame;

use crate::{
    responses::{FromFieldValue, TypedResponseError, value},
    tag::Tag,
};

/// Response to the [`Count`][crate::commands::Count] command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Count {
    /// Number of songs
    pub songs: u64,
    /// Total playtime of the songs
    pub playtime: Duration,
}

impl Count {
    pub(crate) fn from_frame(mut frame: Frame) -> Result<Count, TypedResponseError> {
        Ok(Count {
            songs: value(&mut frame, "songs")?,
            playtime: value(&mut frame, "playtime")?,
        })
    }

    pub(crate) fn from_frame_grouped(
        frame: Frame,
        group_by: &Tag,
    ) -> Result<Vec<(String, Count)>, TypedResponseError> {
        let mut out = Vec::with_capacity(frame.fields_len() / 3);
        build_grouped_values(&mut out, group_by, frame)?;
        Ok(out)
    }
}

fn build_grouped_values<I, V>(
    out: &mut Vec<(String, Count)>,
    grouping_tag: &Tag,
    fields: I,
) -> Result<(), TypedResponseError>
where
    I: IntoIterator<Item = (V, String)>,
    V: AsRef<str>,
{
    let mut fields = fields.into_iter();
    while let Some((key, value)) = fields.next() {
        let mut songs: Option<u64> = None;
        let mut playtime: Option<Duration> = None;

        if key.as_ref() != grouping_tag.as_str() {
            return Err(TypedResponseError::unexpected_field(
                grouping_tag.as_str(),
                key.as_ref(),
            ));
        }

        while songs.is_none() || playtime.is_none() {
            if let Some((key, value)) = fields.next() {
                match key.as_ref() {
                    "songs" => {
                        if songs.is_none() {
                            songs = Some(u64::from_value(value, "songs")?);
                        } else {
                            return Err(TypedResponseError::unexpected_field("playtime", "songs"));
                        }
                    }
                    "playtime" => {
                        if playtime.is_none() {
                            playtime = Some(Duration::from_value(value, "playtime")?);
                        } else {
                            return Err(TypedResponseError::unexpected_field("songs", "playtime"));
                        }
                    }
                    other => {
                        return Err(TypedResponseError::unexpected_field(
                            if songs.is_some() { "playtime" } else { "songs" },
                            other,
                        ));
                    }
                }
            } else {
                return Err(TypedResponseError::missing(if songs.is_some() {
                    "playtime"
                } else {
                    "songs"
                }));
            }
        }

        out.push((
            value,
            Count {
                songs: songs.unwrap(),
                playtime: playtime.unwrap(),
            },
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;

    use super::*;

    #[test]
    fn grouped_values_parsing() {
        let mut out = Vec::new();

        build_grouped_values::<_, &str>(&mut out, &Tag::Album, vec![]).unwrap();
        assert_eq!(out, &[]);

        build_grouped_values(
            &mut out,
            &Tag::Album,
            vec![
                ("Album", String::from("hello")),
                ("songs", String::from("1234")),
                ("playtime", String::from("1234")),
                ("Album", String::from("world")),
                ("playtime", String::from("1")),
                ("songs", String::from("1")),
            ],
        )
        .unwrap();

        assert_eq!(
            out,
            &[
                (
                    String::from("hello"),
                    Count {
                        songs: 1234,
                        playtime: Duration::from_secs(1234)
                    }
                ),
                (
                    String::from("world"),
                    Count {
                        songs: 1,
                        playtime: Duration::from_secs(1)
                    }
                )
            ]
        );
        out.clear();

        let res = build_grouped_values(
            &mut out,
            &Tag::Album,
            vec![
                ("Album", String::from("hello")),
                ("songs", String::from("1234")),
            ],
        );

        assert_matches!(res, Err(_));
    }
}
