use super::KeyValuePair;
use crate::commands::responses::ErrorKind::UnexpectedField;
use crate::commands::responses::{ErrorKind, TypedResponseError};
use std::collections::HashMap;

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
    pub(crate) fn from_frame(
        raw: impl IntoIterator<Item = KeyValuePair>,
    ) -> Result<Self, TypedResponseError> {
        let pair: String = raw.into_iter().map(|(_, value)| value).next().unwrap();

        // server returns the key/value
        // we know the key so just get the value
        let (_, value) = parse_tag(pair)?;

        Ok(Self { value })
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
            .map(|(_, tag): KeyValuePair| parse_tag(tag))
            .collect::<Result<_, _>>();

        Ok(Self { value: value? })
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
        let mut map = HashMap::new();

        let mut file = String::new();

        for (key, tag) in raw {
            match &*key {
                "file" => file = tag,
                "sticker" => {
                    let (_, value) = parse_tag(tag)?;
                    map.insert(file.clone(), value.to_string());
                }
                _ => {
                    return Err(TypedResponseError {
                        field: "sticker",
                        kind: UnexpectedField(key.to_string()),
                    })
                }
            }
        }

        Ok(Self { value: map })
    }
}

/// Parses a `key=value` tag into its key and value strings
fn parse_tag(tag: String) -> Result<(String, String), TypedResponseError> {
    match tag.split_once('=') {
        Some((key, value)) => Ok((key.to_string(), value.to_string())),
        None => Err(TypedResponseError {
            field: "sticker",
            kind: ErrorKind::InvalidValue(tag),
        }),
    }
}
