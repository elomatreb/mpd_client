//! Complete responses.

pub mod error;
pub mod frame;

use bytes::BytesMut;
use fxhash::FxHashSet;
use tracing::trace;

use std::iter::FusedIterator;
use std::option;
use std::slice;
use std::sync::Arc;
use std::vec;

pub use error::Error;
pub use frame::Frame;

/// Response to a command, consisting of an abitrary amount of [frames], which are responses to
/// individual commands, and optionally a single [error].
///
/// Since an error terminates a command list, there can only be one error in a response.
///
/// [frames]: frame::Frame
/// [error]: error::Error
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    /// The sucessful responses.
    frames: Vec<Frame>,
    /// The error, if one occured.
    error: Option<Error>,
}

#[allow(clippy::len_without_is_empty)]
impl Response {
    /// Construct a new response.
    ///
    /// ```
    /// use mpd_protocol::response::{Response, Frame};
    ///
    /// let r = Response::new(vec![Frame::empty()], None);
    /// assert_eq!(1, r.len());
    /// assert!(r.is_success());
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if it is attempted to construct an empty response (i.e. both `frames` and `error`
    /// are empty). This should not occur during normal operation.
    ///
    /// ```should_panic
    /// use mpd_protocol::response::Response;
    ///
    /// // This panics:
    /// Response::new(Vec::new(), None);
    /// ```
    pub fn new(mut frames: Vec<Frame>, error: Option<Error>) -> Self {
        assert!(
            !frames.is_empty() || error.is_some(),
            "attempted to construct an empty (no frames or error) response"
        );

        frames.reverse(); // We want the frames in reverse-chronological order (i.e. oldest last).
        Self { frames, error }
    }

    /// Construct a new "empty" response. This is the simplest possible succesful response,
    /// consisting of a single empty frame.
    ///
    /// ```
    /// use mpd_protocol::response::Response;
    ///
    /// let r = Response::empty();
    /// assert_eq!(1, r.len());
    /// assert!(r.is_success());
    /// ```
    pub fn empty() -> Self {
        Self::new(vec![Frame::empty()], None)
    }

    /// Returns `true` if the response resulted in an error.
    ///
    /// Even if this returns `true`, there may still be succesful frames in the response when the
    /// response is to a command list.
    ///
    /// ```
    /// use mpd_protocol::response::{Response, Error};
    ///
    /// let r = Response::new(Vec::new(), Some(Error::default()));
    /// assert!(r.is_error());
    /// ```
    pub fn is_error(&self) -> bool {
        self.error.is_some()
    }

    /// Returns `true` if the response was entirely succesful (i.e. no errors).
    ///
    /// ```
    /// use mpd_protocol::response::{Response, Frame};
    ///
    /// let r = Response::new(vec![Frame::empty()], None);
    /// assert!(r.is_success());
    /// ```
    pub fn is_success(&self) -> bool {
        !self.is_error()
    }

    /// Get the number of succesful frames in the response.
    ///
    /// May be 0 if the response only consists of an error.
    ///
    /// ```
    /// use mpd_protocol::response::Response;
    ///
    /// let r = Response::empty();
    /// assert_eq!(r.len(), 1);
    /// ```
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// Create an iterator over references to the frames in the response.
    ///
    /// ```
    /// use mpd_protocol::response::{Frame, Response};
    ///
    /// let r = Response::empty();
    /// let mut iter = r.frames();
    ///
    /// assert_eq!(Some(Ok(&Frame::empty())), iter.next());
    /// ```
    pub fn frames(&self) -> FramesRef<'_> {
        FramesRef {
            frames: self.frames.iter(),
            error: self.error.as_ref().into_iter(),
        }
    }

    /// Treat the response as consisting of a single frame or error.
    ///
    /// Frames or errors beyond the first, if they exist, are silently discarded.
    ///
    /// ```
    /// use mpd_protocol::response::{Frame, Response};
    ///
    /// let r = Response::empty();
    /// assert_eq!(Ok(Frame::empty()), r.single_frame());
    /// ```
    pub fn single_frame(self) -> Result<Frame, Error> {
        // There is always at least one frame
        self.into_iter().next().unwrap()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ResponseBuilder {
    field_keys: FxHashSet<Arc<str>>,
    current_frame: Option<Frame>,
    frames: Vec<Frame>,
}

impl ResponseBuilder {
    pub(crate) fn new() -> Self {
        Self {
            field_keys: FxHashSet::default(),
            current_frame: Some(Frame::empty()),
            frames: Vec::new(),
        }
    }

    pub(crate) fn push_field(&mut self, key: &str, value: String) {
        let key = if let Some(v) = self.field_keys.get(key) {
            Arc::clone(v)
        } else {
            let v = Arc::from(key);
            self.field_keys.insert(Arc::clone(&v));
            v
        };

        self.current_frame().fields.push_field(key, value);
    }

    pub(crate) fn push_binary(&mut self, binary: BytesMut) {
        self.current_frame().binary = Some(binary);
    }

    pub(crate) fn finish_frame(&mut self) {
        let frame = self.current_frame.take().unwrap_or_else(Frame::empty);
        trace_frame_completed(&frame);
        self.frames.push(frame);
    }

    pub(crate) fn finish(mut self) -> Response {
        if let Some(frame) = self.current_frame {
            trace_frame_completed(&frame);
            self.frames.push(frame);
        }

        Response {
            frames: self.frames,
            error: None,
        }
    }

    pub(crate) fn error(mut self, error: Error) -> Response {
        if let Some(frame) = self.current_frame {
            trace_frame_completed(&frame);
            self.frames.push(frame);
        }

        Response {
            frames: self.frames,
            error: Some(error),
        }
    }

    fn current_frame(&mut self) -> &mut Frame {
        self.current_frame.get_or_insert_with(Frame::empty)
    }
}

fn trace_frame_completed(frame: &Frame) {
    trace!(
        fields = frame.fields_len(),
        has_binary = frame.has_binary(),
        "completed frame"
    );
}

impl Default for ResponseBuilder {
    fn default() -> Self {
        ResponseBuilder::new()
    }
}

/// Iterator over frames in a response, as returned by [`Response::frames`].
#[derive(Clone, Debug)]
pub struct FramesRef<'a> {
    frames: slice::Iter<'a, Frame>,
    error: option::IntoIter<&'a Error>,
}

impl<'a> Iterator for FramesRef<'a> {
    type Item = Result<&'a Frame, &'a Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(frame) = self.frames.next() {
            Some(Ok(frame))
        } else if let Some(error) = self.error.next() {
            Some(Err(error))
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (mut len, _) = self.frames.size_hint();
        len += self.error.size_hint().0;

        (len, Some(len))
    }
}

impl<'a> FusedIterator for FramesRef<'a> {}
impl<'a> ExactSizeIterator for FramesRef<'a> {}

impl<'a> IntoIterator for &'a Response {
    type Item = Result<&'a Frame, &'a Error>;
    type IntoIter = FramesRef<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.frames()
    }
}

/// Iterator over frames in a response, as returned by [`IntoIterator`] implementation on
/// [`Response`].
#[derive(Clone, Debug)]
pub struct Frames {
    frames: vec::IntoIter<Frame>,
    error: Option<Error>,
}

impl Iterator for Frames {
    type Item = Result<Frame, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(f) = self.frames.next() {
            Some(Ok(f))
        } else if let Some(e) = self.error.take() {
            Some(Err(e))
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        // .len() returns the number of succesful frames, add 1 if there is also an error
        let len = self.frames.len() + if self.error.is_some() { 1 } else { 0 };

        (len, Some(len))
    }
}

impl FusedIterator for Frames {}
impl ExactSizeIterator for Frames {}

impl IntoIterator for Response {
    type Item = Result<Frame, Error>;
    type IntoIter = Frames;

    fn into_iter(self) -> Self::IntoIter {
        Frames {
            frames: self.frames.into_iter(),
            error: self.error,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn owned_frames_iter() {
        let r = Response::new(
            vec![Frame::empty(), Frame::empty(), Frame::empty()],
            Some(Error::default()),
        );

        let mut iter = r.into_iter();

        assert_eq!((4, Some(4)), iter.size_hint());
        assert_eq!(Some(Ok(Frame::empty())), iter.next());

        assert_eq!((3, Some(3)), iter.size_hint());
        assert_eq!(Some(Ok(Frame::empty())), iter.next());

        assert_eq!((2, Some(2)), iter.size_hint());
        assert_eq!(Some(Ok(Frame::empty())), iter.next());

        assert_eq!((1, Some(1)), iter.size_hint());
        assert_eq!(Some(Err(Error::default())), iter.next());

        assert_eq!((0, Some(0)), iter.size_hint());
    }

    #[test]
    fn borrowed_frames_iter() {
        let r = Response::new(
            vec![Frame::empty(), Frame::empty(), Frame::empty()],
            Some(Error::default()),
        );

        let mut iter = r.frames();

        assert_eq!((4, Some(4)), iter.size_hint());
        assert_eq!(Some(Ok(&Frame::empty())), iter.next());

        assert_eq!((3, Some(3)), iter.size_hint());
        assert_eq!(Some(Ok(&Frame::empty())), iter.next());

        assert_eq!((2, Some(2)), iter.size_hint());
        assert_eq!(Some(Ok(&Frame::empty())), iter.next());

        assert_eq!((1, Some(1)), iter.size_hint());
        assert_eq!(Some(Err(&Error::default())), iter.next());

        assert_eq!((0, Some(0)), iter.size_hint());
    }
}
