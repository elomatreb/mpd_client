use super::KeyValuePair;
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
    pub(crate) fn from_frame(raw: impl IntoIterator<Item = KeyValuePair>) -> Self {
        let pair: String = raw.into_iter().map(|(_, value)| value).next().unwrap();

        // server returns the key/value
        // we know the key so just get the value
        let value = pair.split_once('=').unwrap().1.to_string();

        Self { value }
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
    pub(crate) fn from_frame(raw: impl IntoIterator<Item = KeyValuePair>) -> Self {
        let value = raw
            .into_iter()
            .map(|(_, value): KeyValuePair| {
                let split = value.split_once('=').unwrap();
                (split.0.to_string(), split.1.to_string())
            })
            .collect();

        Self { value }
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
    pub(crate) fn from_frame(raw: impl IntoIterator<Item = KeyValuePair>) -> Self {
        let mut map = HashMap::new();

        let mut file = String::new();

        raw.into_iter().for_each(
            |(key, value): KeyValuePair| match key.to_string().as_str() {
                "file" => file = value,
                "sticker" => {
                    let (_, value) = value.split_once('=').unwrap();
                    map.insert(file.clone(), value.to_string());
                }
                _ => panic!("Invalid response received from server"),
            },
        );

        Self { value: map }
    }
}
