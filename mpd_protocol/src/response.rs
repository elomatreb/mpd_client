//! Complete responses.

pub(crate) mod error;
pub mod frame;

use bytes::BytesMut;
use hashbrown::HashSet;

use std::iter::FusedIterator;
use std::mem;
use std::option;
use std::slice;
use std::sync::Arc;
use std::vec;

pub use error::Error;
pub use frame::Frame;

use crate::codec::MpdCodecError;
use crate::parser::ParsedComponent;

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

impl Response {
    /// Construct a new "empty" response. This is the simplest possible succesful response,
    /// consisting of a single empty frame.
    pub(crate) fn empty() -> Self {
        Self {
            frames: Vec::new(),
            error: None,
        }
    }

    /// Returns `true` if the response contains an error.
    ///
    /// Even if this returns `true`, there may still be succesful frames in the response when the
    /// response is to a command list.
    pub fn is_error(&self) -> bool {
        self.error.is_some()
    }

    /// Returns `true` if the response was entirely succesful (i.e. no errors).
    pub fn is_success(&self) -> bool {
        !self.is_error()
    }

    /// Get the number of succesful frames in the response.
    ///
    /// May be 0 if the response only consists of an error.
    pub fn successful_frames(&self) -> usize {
        self.frames.len()
    }

    /// Create an iterator over references to the frames in the response.
    ///
    /// This yields `Result`s, with succesful frames becoming `Ok()`s and an error becoming a
    /// (final) `Err()`.
    pub fn frames(&self) -> FramesRef<'_> {
        FramesRef {
            frames: self.frames.iter(),
            error: self.error.as_ref().into_iter(),
        }
    }

    /// Treat the response as consisting of a single frame or error.
    ///
    /// Frames or errors beyond the first, if they exist, are silently discarded.
    pub fn single_frame(self) -> Result<Frame, Error> {
        // There is always at least one frame
        self.into_iter().next().unwrap()
    }
}

pub(crate) type InternedKeys = HashSet<Arc<str>>;

#[derive(Clone, Debug)]
pub(crate) struct ResponseBuilder {
    fields: InternedKeys,
    state: ResponseState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ResponseState {
    Initial,
    InProgress {
        current: Frame,
    },
    ListInProgress {
        current: Frame,
        completed_frames: Vec<Frame>,
    },
}

impl ResponseBuilder {
    pub(crate) fn new() -> Self {
        Self {
            fields: HashSet::default(),
            state: ResponseState::Initial,
        }
    }

    pub(crate) fn parse(&mut self, src: &mut BytesMut) -> Result<Option<Response>, MpdCodecError> {
        while !src.is_empty() {
            let (remaining, component) = match ParsedComponent::parse(&src, &mut self.fields) {
                Err(e) if e.is_incomplete() => break,
                Err(_) => return Err(MpdCodecError::InvalidMessage),
                Ok(p) => p,
            };

            let msg_end = src.len() - remaining.len();
            let _msg = src.split_to(msg_end);

            match component {
                ParsedComponent::Field { key, value } => self.field(key, value),
                ParsedComponent::BinaryField(binary) => self.binary(binary),
                ParsedComponent::Error(e) => return Ok(Some(self.error(e))),
                ParsedComponent::EndOfFrame => self.finish_frame(),
                ParsedComponent::EndOfResponse => return Ok(Some(self.finish())),
            }
        }

        Ok(None)
    }

    fn field(&mut self, key: Arc<str>, value: String) {
        match &mut self.state {
            ResponseState::Initial => {
                let mut frame = Frame::empty();
                frame.fields.push_field(key, value);
                self.state = ResponseState::InProgress { current: frame };
            }
            ResponseState::InProgress { current }
            | ResponseState::ListInProgress { current, .. } => {
                current.fields.push_field(key, value);
            }
        }
    }

    fn binary(&mut self, binary: BytesMut) {
        match &mut self.state {
            ResponseState::Initial => {
                let mut frame = Frame::empty();
                frame.binary = Some(binary);
                self.state = ResponseState::InProgress { current: frame };
            }
            ResponseState::InProgress { current }
            | ResponseState::ListInProgress { current, .. } => {
                current.binary = Some(binary);
            }
        }
    }

    fn finish_frame(&mut self) {
        let completed_frames = match mem::replace(&mut self.state, ResponseState::Initial) {
            ResponseState::Initial => vec![Frame::empty()],
            ResponseState::InProgress { current } => vec![current],
            ResponseState::ListInProgress {
                current,
                mut completed_frames,
            } => {
                completed_frames.push(current);
                completed_frames
            }
        };

        self.state = ResponseState::ListInProgress {
            current: Frame::empty(),
            completed_frames,
        };
    }

    fn finish(&mut self) -> Response {
        match mem::replace(&mut self.state, ResponseState::Initial) {
            ResponseState::Initial => Response::empty(),
            ResponseState::InProgress { current } => Response {
                frames: vec![current],
                error: None,
            },
            ResponseState::ListInProgress {
                completed_frames, ..
            } => Response {
                frames: completed_frames,
                error: None,
            },
        }
    }

    fn error(&mut self, error: Error) -> Response {
        match mem::replace(&mut self.state, ResponseState::Initial) {
            ResponseState::Initial | ResponseState::InProgress { .. } => Response {
                frames: Vec::new(),
                error: Some(error),
            },
            ResponseState::ListInProgress {
                completed_frames, ..
            } => Response {
                frames: completed_frames,
                error: Some(error),
            },
        }
    }
}

pub(crate) fn intern_key(interned_keys: &mut InternedKeys, key: &str) -> Arc<str> {
    if let Some(k) = interned_keys.get(key) {
        Arc::clone(k)
    } else {
        let k = Arc::from(key);
        interned_keys.insert(Arc::clone(&k));
        k
    }
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
    use assert_matches::assert_matches;

    fn frame<const N: usize>(fields: [(&str, &str); N], binary: Option<&[u8]>) -> Frame {
        let mut out = Frame::empty();

        for &(k, v) in &fields {
            out.fields.push_field(k.into(), v.into());
        }

        out.binary = binary.map(BytesMut::from);

        out
    }

    #[test]
    fn owned_frames_iter() {
        let r = Response {
            frames: vec![Frame::empty(), Frame::empty(), Frame::empty()],
            error: Some(Error::default()),
        };

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
        let r = Response {
            frames: vec![Frame::empty(), Frame::empty(), Frame::empty()],
            error: Some(Error::default()),
        };

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

    #[test]
    fn simple_response() {
        let mut io = BytesMut::from("foo: bar\nOK");

        let mut builder = ResponseBuilder::new();
        assert_eq!(builder.state, ResponseState::Initial);

        // Consume fields
        assert_matches!(builder.parse(&mut io), Ok(None));
        assert_eq!(
            builder.state,
            ResponseState::InProgress {
                current: frame([("foo", "bar")], None)
            }
        );
        assert_eq!(io, "OK");

        // No complete message, do not consume buffer
        assert_matches!(builder.parse(&mut io), Ok(None));
        assert_eq!(
            builder.state,
            ResponseState::InProgress {
                current: frame([("foo", "bar")], None)
            }
        );
        assert_eq!(io, "OK");

        io.extend_from_slice(b"\n");

        // Response now complete
        assert_eq!(
            builder.parse(&mut io).unwrap(),
            Some(Response {
                frames: vec![frame([("foo", "bar")], None)],
                error: None
            })
        );
        assert_eq!(builder.state, ResponseState::Initial);
        assert_eq!(io, "");
    }

    #[test]
    fn response_with_binary() {
        let mut io = BytesMut::from("foo: bar\nbinary: 6\nOK\n");
        let mut builder = ResponseBuilder::new();

        assert_matches!(builder.parse(&mut io), Ok(None));
        assert_eq!(
            builder.state,
            ResponseState::InProgress {
                current: frame([("foo", "bar")], None)
            }
        );
        assert_eq!(io, "binary: 6\nOK\n");

        io.extend_from_slice(b"OK\n\n");

        assert_matches!(builder.parse(&mut io), Ok(None));
        assert_eq!(
            builder.state,
            ResponseState::InProgress {
                current: frame([("foo", "bar")], Some(b"OK\nOK\n")),
            }
        );
        assert_eq!(io, "");

        io.extend_from_slice(b"OK\n");
        assert_eq!(
            builder.parse(&mut io).unwrap(),
            Some(Response {
                frames: vec![frame([("foo", "bar")], Some(b"OK\nOK\n"))],
                error: None,
            })
        );
        assert_eq!(builder.state, ResponseState::Initial);
    }

    #[test]
    fn empty_response() {
        let mut io = BytesMut::from("OK");
        let mut builder = ResponseBuilder::new();

        assert_matches!(builder.parse(&mut io), Ok(None));
        assert_eq!(builder.state, ResponseState::Initial);

        io.extend_from_slice(b"\n");

        assert_eq!(builder.parse(&mut io).unwrap(), Some(Response::empty()));
    }

    #[test]
    fn error() {
        let mut io = BytesMut::from("ACK [5@0] {} unknown command \"foo\"");
        let mut builder = ResponseBuilder::new();

        assert_matches!(builder.parse(&mut io), Ok(None));
        assert_eq!(builder.state, ResponseState::Initial);

        io.extend_from_slice(b"\n");

        assert_eq!(
            builder.parse(&mut io).unwrap(),
            Some(Response {
                frames: vec![],
                error: Some(Error {
                    code: 5,
                    command_index: 0,
                    current_command: None,
                    message: Box::from("unknown command \"foo\""),
                }),
            })
        );
        assert_eq!(builder.state, ResponseState::Initial);
    }

    #[test]
    fn multiple_messages() {
        let mut io = BytesMut::from("foo: bar\nOK\nhello: world\nOK\n");
        let mut builder = ResponseBuilder::new();

        assert_eq!(
            builder.parse(&mut io).unwrap(),
            Some(Response {
                frames: vec![frame([("foo", "bar")], None)],
                error: None
            })
        );
        assert_eq!(io, "hello: world\nOK\n");

        assert_eq!(
            builder.parse(&mut io).unwrap(),
            Some(Response {
                frames: vec![frame([("hello", "world")], None)],
                error: None
            })
        );
        assert_eq!(io, "");
    }

    #[test]
    fn command_list() {
        let mut io = BytesMut::from("foo: bar\n");
        let mut builder = ResponseBuilder::new();

        assert_matches!(builder.parse(&mut io), Ok(None));
        assert_eq!(
            builder.state,
            ResponseState::InProgress {
                current: frame([("foo", "bar")], None)
            }
        );

        io.extend_from_slice(b"list_OK\n");

        assert_matches!(builder.parse(&mut io), Ok(None));
        assert_eq!(
            builder.state,
            ResponseState::ListInProgress {
                current: Frame::empty(),
                completed_frames: vec![frame([("foo", "bar")], None)],
            }
        );

        io.extend_from_slice(b"list_OK\n");

        assert_matches!(builder.parse(&mut io), Ok(None));
        assert_eq!(
            builder.state,
            ResponseState::ListInProgress {
                current: Frame::empty(),
                completed_frames: vec![frame([("foo", "bar")], None), Frame::empty()],
            }
        );

        io.extend_from_slice(b"OK\n");

        assert_eq!(
            builder.parse(&mut io).unwrap(),
            Some(Response {
                frames: vec![frame([("foo", "bar")], None), Frame::empty()],
                error: None
            })
        );
        assert_eq!(builder.state, ResponseState::Initial);
    }

    #[test]
    fn command_list_error() {
        let mut io = BytesMut::from("list_OK\n");
        let mut builder = ResponseBuilder::new();

        assert_matches!(builder.parse(&mut io), Ok(None));
        assert_eq!(
            builder.state,
            ResponseState::ListInProgress {
                current: Frame::empty(),
                completed_frames: vec![Frame::empty()],
            }
        );

        io.extend_from_slice(b"ACK [5@1] {} unknown command \"foo\"\n");

        assert_eq!(
            builder.parse(&mut io).unwrap(),
            Some(Response {
                frames: vec![Frame::empty()],
                error: Some(Error {
                    code: 5,
                    command_index: 1,
                    current_command: None,
                    message: Box::from("unknown command \"foo\""),
                }),
            })
        );
        assert_eq!(builder.state, ResponseState::Initial);
    }

    #[test]
    fn key_interning() {
        let mut io = BytesMut::from("foo: bar\nfoo: baz\nOK\n");

        let mut resp = ResponseBuilder::new()
            .parse(&mut io)
            .expect("incomplete")
            .expect("invalid");

        let mut fields = resp.frames.pop().unwrap().into_iter();

        let (a, _) = fields.next().unwrap();
        let (b, _) = fields.next().unwrap();

        assert!(Arc::ptr_eq(&a, &b));
    }
}
