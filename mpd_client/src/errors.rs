use std::{
    error, fmt,
    num::{ParseFloatError, ParseIntError},
};

use chrono::ParseError;
use mpd_protocol::{
    response::{Error, Frame},
    MpdProtocolError,
};
use tokio::sync::{mpsc::error::SendError, oneshot::error::RecvError};

/// Errors which can occur when issuing a command.
#[derive(Debug)]
pub enum CommandError {
    /// The connection to MPD was closed cleanly
    ConnectionClosed,
    /// An underlying protocol error occurred, including IO errors
    Protocol(MpdProtocolError),
    /// Command returned an error
    ErrorResponse {
        /// The error
        error: Error,
        /// Possible successful frames in the same response, empty if not in a command list
        succesful_frames: Vec<Frame>,
    },
    /// A [typed command](crate::commands) failed to convert its response.
    InvalidTypedResponse(TypedResponseError),
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandError::ConnectionClosed => write!(f, "the connection is closed"),
            CommandError::Protocol(_) => write!(f, "protocol error"),
            CommandError::InvalidTypedResponse(_) => {
                write!(f, "response was invalid for typed command")
            }
            CommandError::ErrorResponse {
                error,
                succesful_frames,
            } => {
                write!(
                    f,
                    "command returned an error [code {}]: {}",
                    error.code, error.message,
                )?;

                if !succesful_frames.is_empty() {
                    write!(f, " (after {} succesful frames)", succesful_frames.len())?;
                }

                Ok(())
            }
        }
    }
}

impl error::Error for CommandError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            CommandError::Protocol(e) => Some(e),
            CommandError::InvalidTypedResponse(e) => Some(e),
            _ => None,
        }
    }
}

#[doc(hidden)]
impl From<MpdProtocolError> for CommandError {
    fn from(e: MpdProtocolError) -> Self {
        CommandError::Protocol(e)
    }
}

#[doc(hidden)]
impl<T> From<SendError<T>> for CommandError {
    fn from(_: SendError<T>) -> Self {
        CommandError::ConnectionClosed
    }
}

#[doc(hidden)]
impl From<RecvError> for CommandError {
    fn from(_: RecvError) -> Self {
        CommandError::ConnectionClosed
    }
}

#[doc(hidden)]
impl From<Error> for CommandError {
    fn from(error: Error) -> Self {
        CommandError::ErrorResponse {
            error,
            succesful_frames: Vec::new(),
        }
    }
}

#[doc(hidden)]
impl From<TypedResponseError> for CommandError {
    fn from(e: TypedResponseError) -> Self {
        CommandError::InvalidTypedResponse(e)
    }
}

/// Errors which may occur while listening for state change events.
#[derive(Debug)]
pub enum StateChangeError {
    /// An underlying protocol error occurred, including IO errors
    Protocol(MpdProtocolError),
    /// The state change message contained an error frame
    ErrorMessage(Error),
}

impl fmt::Display for StateChangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StateChangeError::Protocol(_) => write!(f, "protocol error"),
            StateChangeError::ErrorMessage(Error { code, message, .. }) => write!(
                f,
                "message contained an error frame [code {}]: {}",
                code, message
            ),
        }
    }
}

impl error::Error for StateChangeError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            StateChangeError::Protocol(e) => Some(e),
            _ => None,
        }
    }
}

#[doc(hidden)]
impl From<Error> for StateChangeError {
    fn from(r: Error) -> Self {
        StateChangeError::ErrorMessage(r)
    }
}

#[doc(hidden)]
impl From<MpdProtocolError> for StateChangeError {
    fn from(e: MpdProtocolError) -> Self {
        StateChangeError::Protocol(e)
    }
}

/// Error returned when failing to convert a raw [`Frame`] into the proper typed response.
///
/// [`Frame`]: crate::raw::Frame
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypedResponseError {
    pub(crate) field: &'static str,
    pub(crate) kind: ErrorKind,
}

/// Types of parse errors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ErrorKind {
    /// A required field was missing entirely.
    Missing,
    /// We expected a certain field, but found another.
    UnexpectedField(String),
    /// A field had a unexpected value.
    InvalidValue(String),
    /// A field containing an integer failed to parse.
    MalformedInteger(ParseIntError),
    /// A field containing a float (duration) failed to parse.
    MalformedFloat(ParseFloatError),
    /// A field containing a duration contained an impossible value (e.g. negative or NaN).
    InvalidTimestamp,
    /// A field containing a timestamp failed to parse.
    MalformedTimestamp(ParseError),
}

impl fmt::Display for TypedResponseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            ErrorKind::Missing => write!(f, "field {:?} is required but missing", self.field),
            ErrorKind::InvalidValue(val) => {
                write!(f, "value {:?} is invalid for field {:?}", val, self.field)
            }
            ErrorKind::UnexpectedField(found) => {
                write!(f, "expected field {:?} but found {:?}", self.field, found)
            }
            ErrorKind::MalformedInteger(_) => write!(f, "field {:?} is not an integer", self.field),
            ErrorKind::MalformedFloat(_) | ErrorKind::InvalidTimestamp => {
                write!(f, "field {:?} is not a valid duration", self.field)
            }
            ErrorKind::MalformedTimestamp(_) => {
                write!(f, "field {:?} is not a valid timestamp", self.field)
            }
        }
    }
}

impl error::Error for TypedResponseError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match &self.kind {
            ErrorKind::MalformedFloat(e) => Some(e),
            ErrorKind::MalformedInteger(e) => Some(e),
            ErrorKind::MalformedTimestamp(e) => Some(e),
            _ => None,
        }
    }
}
