//! A succesful response to a command.

use fnv::FnvHashSet;

use std::fmt;
use std::iter::FusedIterator;
use std::slice;
use std::sync::Arc;

/// A succesful response to a command.
///
/// Consists of zero or more key-value pairs, where the keys are not unique, and optionally a
/// single binary blob.
#[derive(Clone, PartialEq, Eq)]
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
    pub fn fields(&self) -> Fields<'_> {
        Fields(self.fields.0.iter())
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

impl fmt::Debug for Frame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            f.debug_struct("Frame")
                .field("fields", &self.fields)
                .field("binary", &self.binary)
                .finish()
        } else {
            f.debug_struct("Frame")
                .field("fields", &self.fields)
                .field(
                    "binary",
                    &self.binary.as_ref().map(|b| format!("<{} bytes>", b.len())),
                )
                .finish()
        }
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
pub(super) struct FieldsContainer(Vec<Option<(Arc<str>, String)>>);

impl fmt::Debug for FieldsContainer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(Fields(self.0.iter())).finish()
    }
}

impl From<&[(&str, &str)]> for FieldsContainer {
    fn from(fields: &[(&str, &str)]) -> Self {
        let mut keys = FnvHashSet::default();

        let fields = fields
            .iter()
            .map(|&(k, v)| Some((simple_intern(&mut keys, k), v.to_owned())))
            .collect();

        Self(fields)
    }
}

fn simple_intern(store: &mut FnvHashSet<Arc<str>>, value: &str) -> Arc<str> {
    match store.get(value) {
        Some(v) => Arc::clone(v),
        None => {
            let v = Arc::from(value);
            store.insert(Arc::clone(&v));
            v
        }
    }
}

/// Iterator returned by the [`fields`] method.
///
/// [`fields`]: struct.Frame.html#method.fields
#[derive(Debug)]
pub struct Fields<'a>(slice::Iter<'a, Option<(Arc<str>, String)>>);

impl<'a> Iterator for Fields<'a> {
    type Item = (&'a str, &'a str);

    fn next(&mut self) -> Option<Self::Item> {
        match self.0.next() {
            None => None,
            Some(None) => self.next(),
            Some(Some((k, v))) => Some((k.as_ref(), v.as_ref())),
        }
    }
}

impl DoubleEndedIterator for Fields<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        match self.0.next_back() {
            None => None,
            Some(None) => self.next_back(),
            Some(Some((k, v))) => Some((k.as_ref(), v.as_ref())),
        }
    }
}

impl FusedIterator for Fields<'_> {}

impl<'a> IntoIterator for &'a Frame {
    type Item = (&'a str, &'a str);
    type IntoIter = Fields<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.fields()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn length() {
        let frame = Frame::empty();
        assert_eq!(frame.fields_len(), 0);

        let frame = Frame {
            fields: FieldsContainer(vec![
                Some((Arc::from("hello"), String::from("world"))),
                Some((Arc::from("foo"), String::from("bar"))),
            ]),
            binary: Some(Vec::from("hello world")),
        };

        assert_eq!(frame.fields_len(), 2);
    }

    #[test]
    fn binary() {
        let mut frame = Frame {
            fields: FieldsContainer::default(),
            binary: Some(Vec::from("hello world")),
        };

        assert!(frame.has_binary());
        assert!(!frame.is_empty());
        assert_eq!(frame.get_binary(), Some(Vec::from("hello world")));
        assert_eq!(frame.get_binary(), None);
        assert!(!frame.has_binary());
    }

    #[test]
    fn accessors() {
        let mut frame = Frame {
            fields: FieldsContainer(vec![
                Some((Arc::from("hello"), String::from("first value"))),
                Some((Arc::from("foo"), String::from("bar"))),
                Some((Arc::from("hello"), String::from("second value"))),
            ]),
            binary: None,
        };

        assert_eq!(frame.find("hello"), Some("first value"));
        assert_eq!(frame.find("404"), None);

        assert_eq!(frame.get("hello"), Some(String::from("first value")));
        assert_eq!(frame.get("hello"), Some(String::from("second value")));
        assert_eq!(frame.get("hello"), None);
    }

    #[test]
    fn iters() {
        let frame = Frame {
            fields: FieldsContainer(vec![
                Some((Arc::from("hello"), String::from("first value"))),
                Some((Arc::from("foo"), String::from("bar"))),
                Some((Arc::from("hello"), String::from("second value"))),
            ]),
            binary: None,
        };
        let mut iter = frame.fields();

        assert_eq!(iter.next(), Some(("hello", "first value")));
        assert_eq!(iter.next(), Some(("foo", "bar")));
        assert_eq!(iter.next(), Some(("hello", "second value")));

        assert_eq!(iter.next(), None);
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn conversion() {
        let fields = vec![
            ("hello", "first value"),
            ("foo", "bar"),
            ("hello", "second value"),
        ];

        let mut converted = FieldsContainer::from(fields.as_slice());

        let c = converted.0.pop().unwrap().unwrap();
        let b = converted.0.pop().unwrap().unwrap();
        let a = converted.0.pop().unwrap().unwrap();

        assert_eq!(a, (Arc::from("hello"), String::from("first value")));
        assert_eq!(b, (Arc::from("foo"), String::from("bar")));
        assert_eq!(c, (Arc::from("hello"), String::from("second value")));

        assert!(Arc::ptr_eq(&a.0, &c.0)); // Same allocation
    }
}
