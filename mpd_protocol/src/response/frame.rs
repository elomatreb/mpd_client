//! A succesful response to a command.

use std::collections::HashMap;
use std::sync::Arc;

/// A succesful response to a command.
///
/// Consists of zero or more key-value pairs, where the keys are not unique, and optionally a
/// single binary blob.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub(super) values: Vec<(Arc<str>, String)>,
    pub(super) binary: Option<Vec<u8>>,
}

impl Frame {
    /// Create an empty frame (0 key-value pairs).
    pub fn empty() -> Self {
        Self {
            values: Vec::new(),
            binary: None,
        }
    }

    /// Get the number of key-value pairs in this response frame.
    pub fn fields_len(&self) -> usize {
        self.values.len()
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

    /// Find the first key-value pair with the given key, and return a reference to its value.
    pub fn find<K>(&self, key: K) -> Option<&str>
    where
        K: AsRef<str>,
    {
        self.values
            .iter()
            .find(|&(k, _)| k.as_ref() == key.as_ref())
            .map(|(_, v)| v.as_str())
    }

    /// Find the first key-value pair with the given key, and return its value.
    ///
    /// This removes it from the list of values in this frame.
    pub fn get<K>(&mut self, key: K) -> Option<String>
    where
        K: AsRef<str>,
    {
        let index = self
            .values
            .iter()
            .enumerate()
            .find(|&(_, (k, _))| k.as_ref() == key.as_ref())
            .map(|(index, _)| index);

        index.map(|i| self.values.remove(i).1)
    }

    /// Get the binary blob contained in this frame, if present.
    ///
    /// This will remove it from the frame, future calls to this method will return `None`.
    pub fn get_binary(&mut self) -> Option<Vec<u8>> {
        self.binary.take()
    }

    /// Collect the key-value pairs in this resposne into a `HashMap`.
    ///
    /// Beware that this loses the order relationship between different keys. Values for a given
    /// key are ordered like they appear in the response.
    pub fn values_as_map(&self) -> HashMap<Arc<str>, Vec<&str>> {
        let mut map = HashMap::new();

        for (k, v) in self.values.iter() {
            map.entry(Arc::clone(k))
                .or_insert_with(Vec::new)
                .push(v.as_str());
        }

        map
    }
}
