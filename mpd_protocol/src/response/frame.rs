//! A succesful response to a command.

use std::sync::Arc;

/// A succesful response to a command.
///
/// Consists of zero or more key-value pairs, where the keys are not unique, and optionally a
/// single binary blob.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub(super) fields: FieldsContainer,
    pub(super) binary: Option<Vec<u8>>,
}

impl Frame {
    /// Create an empty frame (0 key-value pairs).
    pub fn empty() -> Self {
        Self {
            fields: FieldsContainer::default(),
            binary: None,
        }
    }

    /// Get the number of key-value pairs in this response frame.
    pub fn fields_len(&self) -> usize {
        self.fields().count()
    }

    /// Returns `true` if the frame is entirely empty, i.e. contains 0 key-value pairs and no
    /// binary blob.
    pub fn is_empty(&self) -> bool {
        self.fields_len() == 0 && !self.has_binary()
    }

    /// Returns `true` if the frame contains a binary blob.
    ///
    /// This will return `false` after you remove the binary blob using [`get_binary`].
    ///
    /// [`get_binary`]: #method.get_binary
    pub fn has_binary(&self) -> bool {
        self.binary.is_some()
    }

    /// Returns an iterator over all key-value pairs in this frame, in the order they appear in the
    /// response.
    ///
    /// If keys have been removed using [`get`], they will not appear.
    ///
    /// [`get`]: #method.get
    pub fn fields(&self) -> impl Iterator<Item = (&str, &str)> {
        self.fields.0.iter().filter_map(|field| match field {
            None => None,
            Some((key, value)) => Some((key.as_ref(), value.as_ref())),
        })
    }

    /// Find the first key-value pair with the given key, and return a reference to its value.
    pub fn find<K>(&self, key: K) -> Option<&str>
    where
        K: AsRef<str>,
    {
        self.fields()
            .find_map(|(k, v)| if k == key.as_ref() { Some(v) } else { None })
    }

    /// Find the first key-value pair with the given key, and return its value.
    ///
    /// This removes it from the list of fields in this frame.
    pub fn get<K>(&mut self, key: K) -> Option<String>
    where
        K: AsRef<str>,
    {
        self.fields.0.iter_mut().find_map(|field| {
            let k = match field.as_ref() {
                None => return None,
                Some((k, _)) => k,
            };

            if k.as_ref() == key.as_ref() {
                field.take().map(|(_, v)| v)
            } else {
                None
            }
        })
    }

    /// Get the binary blob contained in this frame, if present.
    ///
    /// This will remove it from the frame, future calls to this method will return `None`.
    pub fn get_binary(&mut self) -> Option<Vec<u8>> {
        self.binary.take()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct FieldsContainer(pub(super) Vec<Option<(Arc<str>, String)>>);
