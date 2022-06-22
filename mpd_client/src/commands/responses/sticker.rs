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
        let value = pair.splitn(2, "=").nth(1).unwrap().to_string();

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
                let mut split = value.splitn(2, "=");
                (
                    split.next().unwrap().to_string(),
                    split.next().unwrap().to_string(),
                )
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
        // println!("{:?}", raw.into_iter().collect::<Vec<(KeyValuePair)>>());

        let mut map = HashMap::new();

        let mut file = String::new();

        raw.into_iter().for_each(
            |(key, value): KeyValuePair| match key.to_string().as_str() {
                "file" => file = value,
                "sticker" => {
                    let mut split = value.splitn(2, "=");
                    let value = split.nth(1).unwrap().to_string();
                    map.insert(file.clone(), value);
                }
                _ => panic!("Invalid response received from server"),
            },
        );

        Self { value: map }
    }
}
