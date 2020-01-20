//! Complete responses.

use bytes::Bytes;

use std::collections::HashMap;

/// Response to a command, consisting of an abitrary amount of frames, which are responses to
/// individual commands, and optionally a single error.
///
/// Since an error terminates a command list, there can only be one error in a response.
#[derive(Debug, PartialEq, Eq)]
pub struct Response {
    /// The sucessful responses.
    frames: Vec<Frame>,
    /// The error, if one occured.
    error: Option<Error>,
}

/// Data in a succesful response.
#[derive(Debug, PartialEq, Eq)]
pub struct Frame {
    /// Key-value pairs. Keys can repeat arbitrarily often.
    pub values: Vec<(String, String)>,
    /// Binary frame.
    pub binary: Option<Bytes>,
}

/// Data in an error.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Error {
    /// Error code. See [the MPD
    /// source](https://github.com/MusicPlayerDaemon/MPD/blob/master/src/protocol/Ack.hxx#L30) for
    /// a list of of possible values.
    pub code: u64,
    /// Index of command in a command list that caused this error. 0 when not in a command list.
    pub command_index: u64,
    /// Command that returned the error, if applicable.
    pub current_command: Option<String>,
    /// Message describing the error.
    pub message: String,
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

    /// Get the next frame or error from the response as a `Result`.
    ///
    /// Includes the remaining portion of the response. A possible error always occurs as the last
    /// element, since it terminates command lists.
    ///
    /// ```
    /// use mpd_protocol::response::{Response, Frame};
    ///
    /// let r = Response::new(vec![Frame::empty(), Frame::empty()], None);
    /// let res = r.next_frame().unwrap();
    /// assert_eq!(Frame::empty(), res.0);
    ///
    /// assert_eq!(Ok((Frame::empty(), None)), res.1.unwrap().next_frame());
    /// ```
    pub fn next_frame(mut self) -> Result<(Frame, Option<Self>), Error> {
        match self.frames.pop() {
            Some(frame) => {
                let remaining = if !self.frames.is_empty() || self.error.is_some() {
                    Some(self)
                } else {
                    None
                };

                Ok((frame, remaining))
            }

            None => Err(self.error.unwrap()),
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
        self.next_frame().map(|(f, _)| f)
    }

    /// Creates an iterator over all frames and errors in the response.
    ///
    /// ```
    /// use mpd_protocol::response::{Frame, Response};
    ///
    /// let mut first = vec![(String::from("hello"), String::from("world"))];
    ///
    /// let second = vec![(String::from("foo"), String::from("bar"))];
    ///
    /// let mut iter = Response::new(vec![Frame {
    ///     values: first.clone(),
    ///     binary: None,
    /// }, Frame {
    ///     values: second.clone(),
    ///     binary: None,
    /// }], None).frames();
    ///
    /// assert_eq!(Some(Ok(Frame {
    ///     values: first,
    ///     binary: None,
    /// })), iter.next());
    ///
    /// assert_eq!(Some(Ok(Frame {
    ///     values: second,
    ///     binary: None,
    /// })), iter.next());
    ///
    /// assert_eq!(None, iter.next());
    /// ```
    pub fn frames(self) -> impl Iterator<Item = Result<Frame, Error>> {
        Frames(Some(self))
    }
}

struct Frames(Option<Response>);

impl Iterator for Frames {
    type Item = Result<Frame, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.0.take() {
            None => None,
            Some(r) => Some(match r.next_frame() {
                Err(e) => Err(e),
                Ok((value, remaining)) => {
                    self.0 = remaining;
                    Ok(value)
                }
            }),
        }
    }
}

impl Frame {
    /// Create an empty frame (0 key-value pairs).
    ///
    /// ```
    /// use mpd_protocol::response::Frame;
    ///
    /// let f = Frame::empty();
    /// assert_eq!(0, f.values.len());
    /// assert!(f.binary.is_none());
    /// ```
    pub fn empty() -> Self {
        Self {
            values: Vec::new(),
            binary: None,
        }
    }

    /// Collect the key-value pairs in this resposne into a `HashMap`.
    ///
    /// Beware that this loses the order relationship between different keys. Values for a given
    /// key are ordered like they appear in the response.
    ///
    /// ```
    /// use mpd_protocol::response::Frame;
    ///
    /// let f = Frame {
    ///     values: vec![
    ///         (String::from("foo"), String::from("bar")),
    ///         (String::from("hello"), String::from("world")),
    ///         (String::from("foo"), String::from("baz")),
    ///     ],
    ///     binary: None,
    /// };
    ///
    /// let map = f.values_as_map();
    ///
    /// assert_eq!(map.get("foo"), Some(&vec!["bar", "baz"]));
    /// assert_eq!(map.get("hello"), Some(&vec!["world"]));
    /// ```
    pub fn values_as_map(&self) -> HashMap<&str, Vec<&str>> {
        let mut map = HashMap::new();

        for (k, v) in self.values.iter() {
            map.entry(k.as_str())
                .or_insert_with(Vec::new)
                .push(v.as_str());
        }

        map
    }
}
