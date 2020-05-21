//! Complete responses.

pub mod error;
pub mod frame;

use std::convert::TryFrom;
use std::fmt;
use std::iter::FusedIterator;

use crate::parser;

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

/// Errors returned when attmepting to construct an owned [`Response`] from a list of parser results
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnedResponseError {
    /// There were further frames after an error frame
    FramesAfterError,
    /// An empty slice was provided (A response needs at least one frame or error)
    Empty,
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
            response: self,
            frames_cursor: 0,
            error_consumed: false,
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
        self.into_frames().next().unwrap()
    }

    /// Creates an iterator over all frames and errors in the response.
    pub fn into_frames(self) -> Frames {
        Frames(self)
    }
}

impl<'a> TryFrom<&'a [parser::Response<'_>]> for Response {
    type Error = OwnedResponseError;

    fn try_from(raw_frames: &'a [parser::Response<'_>]) -> Result<Self, Self::Error> {
        if raw_frames.is_empty() {
            return Err(OwnedResponseError::Empty);
        }

        // Optimistically pre-allocated Vec
        let mut frames = Vec::with_capacity(raw_frames.len());
        let mut error = None;

        for frame in raw_frames.iter().rev() {
            match frame {
                parser::Response::Success { fields, binary } => {
                    let binary = binary.map(Vec::from);

                    frames.push(Frame {
                        fields: frame::FieldsContainer::from(fields.as_slice()),
                        binary,
                    });
                }
                parser::Response::Error {
                    code,
                    command_index,
                    current_command,
                    message,
                } => {
                    if !frames.is_empty() {
                        // If we already saw succesful frames, the error would not have been the
                        // final element
                        return Err(OwnedResponseError::FramesAfterError);
                    }

                    error = Some(Error {
                        code: *code,
                        command_index: *command_index,
                        current_command: current_command.map(String::from),
                        message: (*message).to_owned(),
                    });
                }
            }
        }

        Ok(Response { frames, error })
    }
}

/// Iterator over frames in a response, as returned by [`Response::frames`].
#[derive(Copy, Clone, Debug)]
pub struct FramesRef<'a> {
    response: &'a Response,
    frames_cursor: usize,
    error_consumed: bool,
}

impl<'a> Iterator for FramesRef<'a> {
    type Item = Result<&'a Frame, &'a Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.frames_cursor < self.response.frames.len() {
            let frame = self.response.frames.get(self.frames_cursor).unwrap();
            self.frames_cursor += 1;
            Some(Ok(frame))
        } else if !self.error_consumed {
            self.error_consumed = true;
            self.response.error.as_ref().map(Err)
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let mut len = self.response.frames.len() - self.frames_cursor;

        if !self.error_consumed && self.response.is_error() {
            len += 1;
        }

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

/// Iterator over frames in a response, as returned by [`Response::into_frames`].
#[derive(Clone, Debug)]
pub struct Frames(Response);

impl Iterator for Frames {
    type Item = Result<Frame, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(frame) = self.0.frames.pop() {
            Some(Ok(frame))
        } else if let Some(error) = self.0.error.take() {
            Some(Err(error))
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        // .len() returns the number of succesful frames, add 1 if there is also an error
        let len = self.0.len() + if self.0.is_error() { 1 } else { 0 };

        (len, Some(len))
    }
}

impl FusedIterator for Frames {}
impl ExactSizeIterator for Frames {}

impl IntoIterator for Response {
    type Item = Result<Frame, Error>;
    type IntoIter = Frames;

    fn into_iter(self) -> Self::IntoIter {
        self.into_frames()
    }
}
impl fmt::Display for OwnedResponseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OwnedResponseError::FramesAfterError => {
                write!(f, "Error frame was not the final element of response")
            }
            OwnedResponseError::Empty => {
                write!(f, "Attempted to construct response with no values")
            }
        }
    }
}

impl std::error::Error for OwnedResponseError {}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn owned_frames_iter() {
        let r = Response::new(
            vec![Frame::empty(), Frame::empty(), Frame::empty()],
            Some(Error::default()),
        );

        let mut iter = r.into_frames();

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
