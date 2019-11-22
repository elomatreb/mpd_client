use bytes::{BufMut, BytesMut};
use tokio_util::codec::{Decoder, Encoder};

use std::error::Error;
use std::fmt;
use std::io;
use std::str;

use crate::response::Response;

/// Codec for MPD protocol.
#[derive(Debug, Default)]
pub struct MpdCodec {
    examined_up_to: usize,
    parsing_error: bool,
    greeted: bool,
}

impl MpdCodec {
    /// Creates a new MpdCodec
    pub fn new() -> Self {
        MpdCodec::default()
    }

    /// Creates a new MpdCodec that does not expect a server greeting
    pub fn new_greeted() -> Self {
        Self {
            greeted: true,
            ..Default::default()
        }
    }
}

impl Encoder for MpdCodec {
    type Item = String;
    type Error = MpdCodecError;

    fn encode(&mut self, command: Self::Item, buf: &mut BytesMut) -> Result<(), Self::Error> {
        if command.is_empty() || command.contains('\n') {
            return Err(MpdCodecError::InvalidCommand(command));
        }

        // Commands are simply delimited by a newline
        buf.reserve(command.len() + 1);
        buf.put(command);
        buf.put("\n");

        Ok(())
    }
}

impl Decoder for MpdCodec {
    type Item = Response;
    type Error = MpdCodecError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        unimplemented!()
    }
}

/// Errors which can occur during operation
#[derive(Debug)]
pub enum MpdCodecError {
    /// IO error occured
    Io(io::Error),
    /// A line wasn't a "key: value"
    InvalidKeyValueSequence,
    /// A line started like an error but wasn't correctly formatted
    InvalidErrorMessage,
    /// A message contained invalid unicode
    InvalidEncoding(str::Utf8Error),
    /// Did not get expected greeting as first message (`OK MPD <protocol version>`)
    InvalidGreeting,
    /// A command string passed to the encoder was invalid (empty or contained a newline)
    InvalidCommand(String),
}

impl fmt::Display for MpdCodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MpdCodecError::InvalidKeyValueSequence => {
                write!(f, "response contained invalid key-sequence")
            }
            MpdCodecError::InvalidErrorMessage => {
                write!(f, "response contained invalid error message")
            }
            MpdCodecError::InvalidCommand(command) => write!(f, "invalid command: {:?}", command),
            MpdCodecError::InvalidGreeting => write!(f, "did not receive expected greeting"),
            MpdCodecError::InvalidEncoding(e) => write!(f, "{}", e),
            MpdCodecError::Io(e) => write!(f, "{}", e),
        }
    }
}

impl From<io::Error> for MpdCodecError {
    fn from(e: io::Error) -> Self {
        MpdCodecError::Io(e)
    }
}

impl From<str::Utf8Error> for MpdCodecError {
    fn from(e: str::Utf8Error) -> Self {
        MpdCodecError::InvalidEncoding(e)
    }
}

impl Error for MpdCodecError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            MpdCodecError::Io(e) => Some(e),
            MpdCodecError::InvalidEncoding(e) => Some(e),
            _ => None,
        }
    }
}
