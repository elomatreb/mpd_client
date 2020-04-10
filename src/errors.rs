//! Errors.

use mpd_protocol::{
    response::{Error as ErrorResponse, Frame},
    MpdCodecError,
};
use tokio::sync::{mpsc::error::SendError, oneshot::error::RecvError};

use std::error::Error;
use std::fmt;

/// Errors which can occur when issuing a command.
#[derive(Debug)]
pub enum CommandError {
    /// The connection to MPD is closed
    ConnectionClosed,
    /// Received or attempted to send an invalid message
    InvalidMessage(MpdCodecError),
    /// Command returned an error
    ErrorResponse {
        /// The error
        error: ErrorResponse,
        /// Possible sucessful frames in the same response, empty if not in a command list
        succesful_frames: Vec<Frame>,
    },
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandError::ConnectionClosed => write!(f, "The connection is closed"),
            CommandError::InvalidMessage(_) => write!(f, "Invalid message"),
            CommandError::ErrorResponse {
                error,
                succesful_frames,
            } => write!(
                f,
                "Command returned an error (code {} - {:?}) after {} succesful frames",
                error.code,
                error.message,
                succesful_frames.len()
            ),
        }
    }
}

impl Error for CommandError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            CommandError::InvalidMessage(e) => Some(e),
            _ => None,
        }
    }
}

#[doc(hidden)]
impl From<MpdCodecError> for CommandError {
    fn from(e: MpdCodecError) -> Self {
        CommandError::InvalidMessage(e)
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
impl From<ErrorResponse> for CommandError {
    fn from(error: ErrorResponse) -> Self {
        CommandError::ErrorResponse {
            error,
            succesful_frames: Vec::new(),
        }
    }
}

/// Errors which may occur while listening for state change events.
#[derive(Debug)]
pub enum StateChangeError {
    /// The message was invalid
    InvalidMessage(MpdCodecError),
    /// The state change message contained an error frame
    ErrorMessage(ErrorResponse),
    /// The state message wasn't empty, but did not contain the expected `changed` key
    MissingChangedKey,
}

impl fmt::Display for StateChangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StateChangeError::InvalidMessage(_) => write!(f, "Invalid message"),
            StateChangeError::ErrorMessage(ErrorResponse { code, message, .. }) => write!(
                f,
                "Message contained an error frame (code {} - {:?})",
                code, message
            ),
            StateChangeError::MissingChangedKey => write!(f, "Message was missing 'changed' key"),
        }
    }
}

impl Error for StateChangeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            StateChangeError::InvalidMessage(e) => Some(e),
            _ => None,
        }
    }
}

#[doc(hidden)]
impl From<ErrorResponse> for StateChangeError {
    fn from(r: ErrorResponse) -> Self {
        StateChangeError::ErrorMessage(r)
    }
}

#[doc(hidden)]
impl From<MpdCodecError> for StateChangeError {
    fn from(e: MpdCodecError) -> Self {
        StateChangeError::InvalidMessage(e)
    }
}
