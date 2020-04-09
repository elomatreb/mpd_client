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
