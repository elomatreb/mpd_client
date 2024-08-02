use std::collections::HashMap;
use std::mem;
use std::time::Duration;
use mpd_protocol::response::Frame;
use crate::responses::{FromFieldValue, Timestamp, TypedResponseError};
use crate::tag::Tag;

/// A single storage item, as returned by the commands listall, listallinfo, listfiles, lsinfo.
///
///[getItems]: crate::commands::definitions::GetItems
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct StorageItem {
    pub item_type: String,
    pub name: String,
    /// Tags in this response.
    pub tags: HashMap<Tag, Vec<String>>,
    /// The `duration` as returned by MPD.
    pub duration: Option<Duration>,
    /// The `format` as returned by MPD.
    pub format: Option<String>,
    /// Last modification date of the underlying file.
    pub last_modified: Option<Timestamp>,
}

impl StorageItem {
    pub fn from_frame_multi(frame: Frame) -> Result<Vec<StorageItem>, TypedResponseError> {
        let mut out = Vec::new();
        let mut builder = StorageItemBuilder::default();

        for (key, value) in frame {
            if let Some(storage_item) = builder.field(&key, value)? {
                out.push(storage_item);
            }
        }

        if let Some(storage_item) = builder.finish() {
            out.push(storage_item);
        }

        Ok(out)
    }
}



#[derive(Debug, Default)]
struct StorageItemBuilder {
    item_type: String,
    name: String,
    tags: HashMap<Tag, Vec<String>>,
    duration: Option<Duration>,
    format: Option<String>,
    last_modified: Option<Timestamp>,
}

impl StorageItemBuilder {
    /// Handle a field from a storage item list.
    ///
    /// If this returns `Ok(Some(_))`, a storage item was completed and another one started.
    fn field(
        &mut self,
        key: &str,
        value: String,
    ) -> Result<Option<StorageItem>, TypedResponseError> {
        if self.item_type.is_empty() {
            // No storage item is currently in progress
            self.handle_start_field(key, value)?;
            Ok(None)
        } else {
            // Currently parsing a storage item
            self.handle_storage_item_field(key, value)
        }
    }

    /// Handle a field that is expected to start a new storage item.
    fn handle_start_field(&mut self, key: &str, value: String) -> Result<(), TypedResponseError> {
        match key {
            // A 'file' | 'directory' | 'playlist' field starts a new storage item
            "file" | "directory" | "playlist"  => {
                self.name = value;
                self.item_type = key.to_string()
            },
            // Any other fields are invalid
            other => return Err(TypedResponseError::unexpected_field("file | directory | playlist", other)),
        }

        Ok(())
    }

    /// Handle a field that may be part of a storage item or may start a new one.
    fn handle_storage_item_field(
        &mut self,
        key: &str,
        value: String,
    ) -> Result<Option<StorageItem>, TypedResponseError> {
        // If this field starts a new storage item, the current one is done
        if is_start_field(key) {
            // Reset the storage item builder and convert the existing data into a storage item
            let storage_item = mem::take(self).into_storage_item();

            // Handle the current field
            self.handle_start_field(key, value)?;

            // Return the complete storage item
            return Ok(Some(storage_item));
        }

        // The field is a component of a storage item
        match key {
            "duration" => self.duration = Some(Duration::from_value(value, "duration")?),
            "Time" => {
                // Just a worse `duration` field, but retained for backwards compatibility with
                // protocol versions <0.20
                if self.duration.is_none() {
                    self.duration = Some(Duration::from_value(value, "Time")?);
                }
            }
            "Format" => self.format = Some(value),
            "Last-Modified" => {
                let lm = Timestamp::from_value(value, "Last-Modified")?;
                self.last_modified = Some(lm);
            }
            tag => {
                // Anything else is a tag.
                // It's fine to unwrap here because the protocol implementation already validated
                // the field name
                let tag = Tag::try_from(tag).unwrap();
                self.tags.entry(tag).or_default().push(value);
            }
        }

        Ok(None)
    }

    /// Finish the building process. This returns the final storage item, if there is one.
    fn finish(self) -> Option<StorageItem> {
        if self.item_type.is_empty() {
            None
        } else {
            Some(self.into_storage_item())
        }
    }

    fn into_storage_item(self) -> StorageItem{
        assert!(!self.item_type.is_empty());
        StorageItem {
            item_type: self.item_type,
            name: self.name,
            tags: self.tags,
            duration: self.duration,
            format: self.format,
            last_modified: self.last_modified,
        }
    }
}

/// Returns `true` if the given field name starts a new storage item entry.
fn is_start_field(f: &str) -> bool {
    matches!(f, "file" | "directory" | "playlist")
}