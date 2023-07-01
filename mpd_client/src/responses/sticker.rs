use std::collections::HashMap;

use mpd_protocol::response::Frame;

use crate::responses::{KeyValuePair, TypedResponseError};

/// Response to the [`sticker get`] command.
///
/// [`sticker get`]: crate::commands::definitions::StickerGet
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct StickerGet {
    /// The sticker value
    pub value: String,
}

impl StickerGet {
    pub(crate) fn from_frame(frame: Frame) -> Result<Self, TypedResponseError> {
        let Some((key, field_value)) = frame.into_iter().next() else {
            return Err(TypedResponseError::missing("sticker"));
        };

        if &*key != "sticker" {
            return Err(TypedResponseError::unexpected_field(
                "sticker",
                key.as_ref(),
            ));
        }

        let (_, sticker_value) = parse_sticker_value(field_value)?;

        Ok(StickerGet {
            value: sticker_value,
        })
    }
}

impl From<StickerGet> for String {
    fn from(sticker_get: StickerGet) -> Self {
        sticker_get.value
    }
}

/// Response to the [`sticker list`] command.
///
/// [`sticker list`]: crate::commands::definitions::StickerList
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct StickerList {
    /// A map of sticker names to their values
    pub value: HashMap<String, String>,
}

impl StickerList {
    pub(crate) fn from_frame(
        raw: impl IntoIterator<Item = KeyValuePair>,
    ) -> Result<Self, TypedResponseError> {
        let value = raw
            .into_iter()
            .map(|(_, value)| parse_sticker_value(value))
            .collect::<Result<_, _>>()?;

        Ok(Self { value })
    }
}

impl From<StickerList> for HashMap<String, String> {
    fn from(sticker_list: StickerList) -> Self {
        sticker_list.value
    }
}

/// Response to the [`sticker find`] command.
///
/// [`sticker find`]: crate::commands::definitions::StickerFind
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct StickerFind {
    /// A map of songs to their sticker values
    pub value: HashMap<String, String>,
}

impl StickerFind {
    pub(crate) fn from_frame(
        raw: impl IntoIterator<Item = KeyValuePair>,
    ) -> Result<Self, TypedResponseError> {
        let mut value = HashMap::new();

        let mut file = String::new();

        for (key, tag) in raw {
            match &*key {
                "file" => file = tag,
                "sticker" => {
                    let (_, sticker_value) = parse_sticker_value(tag)?;
                    value.insert(file.clone(), sticker_value);
                }
                other => return Err(TypedResponseError::unexpected_field("sticker", other)),
            }
        }

        Ok(Self { value })
    }
}

/// Parses a `key=value` tag into its key and value strings
fn parse_sticker_value(mut tag: String) -> Result<(String, String), TypedResponseError> {
    match tag.split_once('=') {
        Some((key, value)) => {
            let value = String::from(value);
            tag.truncate(key.len());
            Ok((tag, value))
        }
        None => Err(TypedResponseError::invalid_value("sticker", tag)),
    }
}
