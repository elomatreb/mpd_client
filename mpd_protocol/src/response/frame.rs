//! A succesful response to a command.

use bytes::BytesMut;

use std::fmt;
use std::iter::FusedIterator;
use std::slice;
use std::sync::Arc;
use std::vec;

/// A succesful response to a command.
///
/// Consists of zero or more key-value pairs, where the keys are not unique, and optionally a
/// single binary blob.
#[derive(Clone, PartialEq, Eq)]
pub struct Frame {
    pub(super) fields: FieldsContainer,
    pub(super) binary: Option<BytesMut>,
}

impl Frame {
    /// Create an empty frame (0 key-value pairs).
    pub(crate) fn empty() -> Self {
        Self {
            fields: FieldsContainer(Vec::new()),
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
    /// If the binary blob has been removed using [`Frame::get_binary`], this will return `false`.
    pub fn has_binary(&self) -> bool {
        self.binary.is_some()
    }

    /// Returns an iterator over all key-value pairs in this frame, in the order they appear in the
    /// response.
    ///
    /// If keys have been removed using [`Frame::get`], they will not appear.
    pub fn fields(&self) -> Fields<'_> {
        Fields(self.fields.0.iter())
    }

    /// Find the first key-value pair with the given key, and return a reference to its value.
    ///
    /// The key is case-sensitive.
    pub fn find<K>(&self, key: K) -> Option<&str>
    where
        K: AsRef<str>,
    {
        self.fields()
            .find_map(|(k, v)| if k == key.as_ref() { Some(v) } else { None })
    }

    /// Returns a reference to the binary blob in this frame, if there is one.
    ///
    /// If the binary blob has been removed using [`Frame::get_binary`], this will return `None`.
    pub fn binary(&self) -> Option<&[u8]> {
        self.binary.as_deref()
    }

    /// Find the first key-value pair with the given key, and return its value.
    ///
    /// The key is case-sensitive. This removes it from the list of fields in this frame.
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
    pub fn get_binary(&mut self) -> Option<BytesMut> {
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

#[derive(Clone, PartialEq, Eq)]
pub(super) struct FieldsContainer(Vec<Option<(Arc<str>, String)>>);

impl FieldsContainer {
    pub(super) fn push_field(&mut self, key: Arc<str>, value: String) {
        self.0.push(Some((key, value)));
    }
}

impl fmt::Debug for FieldsContainer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(Fields(self.0.iter())).finish()
    }
}

/// Iterator returned by the [`Frame::fields`] method.
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

/// Iterator returned by the [`IntoIterator`] implementation on [`Frame`].
#[derive(Debug)]
pub struct IntoIter {
    iter: vec::IntoIter<Option<(Arc<str>, String)>>,
    binary: Option<BytesMut>,
}

impl IntoIter {
    /// Get the binary blob contained in this frame, if present.
    ///
    /// This will remove it from the frame, future calls to this method will return `None`.
    pub fn get_binary(&mut self) -> Option<BytesMut> {
        self.binary.take()
    }
}

impl Iterator for IntoIter {
    type Item = (Arc<str>, String);

    fn next(&mut self) -> Option<Self::Item> {
        match self.iter.next() {
            None => None,
            Some(None) => self.next(),
            Some(value) => value,
        }
    }
}

impl DoubleEndedIterator for IntoIter {
    fn next_back(&mut self) -> Option<Self::Item> {
        match self.iter.next_back() {
            None => None,
            Some(None) => self.next_back(),
            Some(value) => value,
        }
    }
}

impl FusedIterator for IntoIter {}

impl IntoIterator for Frame {
    type Item = (Arc<str>, String);
    type IntoIter = IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        IntoIter {
            iter: self.fields.0.into_iter(),
            binary: self.binary,
        }
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
            binary: Some(BytesMut::from("hello world")),
        };

        assert_eq!(frame.fields_len(), 2);
    }

    #[test]
    fn binary() {
        let mut frame = Frame {
            fields: FieldsContainer(Vec::new()),
            binary: Some(BytesMut::from("hello world")),
        };

        assert!(frame.has_binary());
        assert!(!frame.is_empty());
        assert_eq!(frame.get_binary(), Some(BytesMut::from("hello world")));
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
        assert_eq!(frame.find("HELLO"), None); // case-sensitive

        assert_eq!(frame.get("hello"), Some(String::from("first value")));
        assert_eq!(frame.get("hello"), Some(String::from("second value")));
        assert_eq!(frame.get("hello"), None);
        assert_eq!(frame.get("Foo"), None); // case-sensitive
    }

    #[test]
    fn iter() {
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
    fn owned_iter() {
        let frame = Frame {
            fields: FieldsContainer(vec![
                Some((Arc::from("hello"), String::from("first value"))),
                Some((Arc::from("foo"), String::from("bar"))),
                Some((Arc::from("hello"), String::from("second value"))),
            ]),
            binary: None,
        };
        let mut iter = frame.into_iter();

        assert_eq!(iter.next(), Some(("hello".into(), "first value".into())));
        assert_eq!(iter.next(), Some(("foo".into(), "bar".into())));
        assert_eq!(iter.next(), Some(("hello".into(), "second value".into())));

        assert_eq!(iter.next(), None);
        assert_eq!(iter.next(), None);
    }
}
