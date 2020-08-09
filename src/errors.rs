use tokio::sync::{mpsc::error::SendError, oneshot::error::RecvError};

use std::error::Error;
use std::fmt;

use crate::commands::responses::TypedResponseError;
use crate::raw::{ErrorResponse, Frame, ProtocolError};

/// Errors which can occur when issuing a command.
#[derive(Debug)]
pub enum CommandError {
    /// The connection to MPD was closed cleanly
    ConnectionClosed,
    /// An underlying protocol error occured, including IO errors
    Protocol(ProtocolError),
    /// Command returned an error
    ErrorResponse {
        /// The error
        error: ErrorResponse,
        /// Possible sucessful frames in the same response, empty if not in a command list
        succesful_frames: Vec<Frame>,
    },
    /// A [typed command](crate::commands) failed to convert its response.
    InvalidTypedResponse(TypedResponseError),
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandError::ConnectionClosed => write!(f, "The connection is closed"),
            CommandError::Protocol(_) => write!(f, "Protocol error"),
            CommandError::InvalidTypedResponse(_) => {
                write!(f, "Response was invalid for typed command")
            }
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
            CommandError::Protocol(e) => Some(e),
            CommandError::InvalidTypedResponse(e) => Some(e),
            _ => None,
        }
    }
}

#[doc(hidden)]
impl From<ProtocolError> for CommandError {
    fn from(e: ProtocolError) -> Self {
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
impl From<ErrorResponse> for CommandError {
    fn from(error: ErrorResponse) -> Self {
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
    /// An underlying protocol error occured, including IO errors
    Protocol(ProtocolError),
    /// The state change message contained an error frame
    ErrorMessage(ErrorResponse),
}

impl fmt::Display for StateChangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StateChangeError::Protocol(_) => write!(f, "Protocol error"),
            StateChangeError::ErrorMessage(ErrorResponse { code, message, .. }) => write!(
                f,
                "Message contained an error frame (code {} - {:?})",
                code, message
            ),
        }
    }
}

impl Error for StateChangeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            StateChangeError::Protocol(e) => Some(e),
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
impl From<ProtocolError> for StateChangeError {
    fn from(e: ProtocolError) -> Self {
        StateChangeError::Protocol(e)
    }
}
