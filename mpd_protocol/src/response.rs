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
    pub frames: Vec<Frame>,
    /// The error, if one occured.
    pub error: Option<Error>,
}

/// Data in a succesful response.
#[derive(Debug, Default, PartialEq, Eq)]
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

impl Response {
    /// Construct a new response.
    ///
    /// ```
    /// use mpd_protocol::response::{Response, Frame};
    ///
    /// let r = Response::new(vec![Frame::default()], None);
    /// assert_eq!(1, r.frames.len());
    /// assert!(r.error.is_none());
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
    pub fn new(frames: Vec<Frame>, error: Option<Error>) -> Self {
        assert!(
            !frames.is_empty() || error.is_some(),
            "attempted to construct an empty (no frames or error) response"
        );

        Self { frames, error }
    }

    /// Construct a new "empty" response. This is the simplest possible succesful response,
    /// consisting of a single empty frame.
    ///
    /// ```
    /// use mpd_protocol::response::Response;
    ///
    /// let r = Response::empty();
    /// assert_eq!(1, r.frames.len());
    /// assert_eq!(None, r.error);
    /// ```
    pub fn empty() -> Self {
        Self::new(vec![Default::default()], None)
    }

    /// Returns `true` if the response resulted in an error.
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
    /// let r = Response::new(vec![Frame::default()], None);
    /// assert!(r.is_success());
    /// ```
    pub fn is_success(&self) -> bool {
        !self.is_error()
    }
}

impl Frame {
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
    ///     ..Default::default()
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
