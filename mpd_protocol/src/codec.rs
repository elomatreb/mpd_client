use bytes::{BufMut, Bytes, BytesMut};
use tokio_util::codec::{Decoder, Encoder};

use std::error::Error;
use std::fmt;
use std::io;

use crate::response::{parser, Error as ResponseError, Frame, Response};

/// Codec for MPD protocol.
#[derive(Debug, Default)]
pub struct MpdCodec {
    cursor: usize,
    protocol_version: Option<String>,
}

impl MpdCodec {
    /// Creates a new MpdCodec
    pub fn new() -> Self {
        MpdCodec::default()
    }

    /// Returns the protocol version the server is speaking, if this decoder instance already
    /// received a greeting, `None` otherwise.
    pub fn protocol_version(&self) -> Option<&str> {
        self.protocol_version.as_ref().map(String::as_str)
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
        buf.put(command.as_bytes());
        buf.put_u8(b'\n');

        Ok(())
    }
}

impl Decoder for MpdCodec {
    type Item = Response;
    type Error = MpdCodecError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if self.protocol_version.is_none() {
            match parser::greeting(src) {
                Ok((rem, greeting)) => {
                    self.protocol_version = Some(greeting.version.to_owned());

                    // Drop the part of the buffer containing the greeting
                    let new_start = src.len() - rem.len();
                    src.split_to(new_start);
                }
                Err(e) => {
                    if e.is_incomplete() {
                        return Ok(None);
                    } else {
                        // We got a malformed greeting
                        return Err(MpdCodecError::InvalidGreeting(src.split().freeze()));
                    }
                }
            }
        }

        for (terminator, _) in src[self.cursor..]
            .windows(3)
            .enumerate()
            .filter(|(_, w)| w == b"OK\n")
        {
            let msg_end = self.cursor + terminator + 3;

            match parser::response(&src[..msg_end]) {
                Ok((_remainder, response)) => {
                    let r = convert_raw_response(&response);
                    src.split_to(msg_end);
                    return Ok(Some(r));
                }
                Err(e) => {
                    if !e.is_incomplete() {
                        return Err(MpdCodecError::InvalidResponse(src.split().freeze()));
                    }
                }
            }
        }

        // We didn't find a terminator

        // Subtract two in case the terminator was already partially in the buffer
        self.cursor = src.len().saturating_sub(2);

        Ok(None)
    }
}

/// Convert the raw parsed response to one with owned data
fn convert_raw_response(res: &[parser::Response]) -> Response {
    let mut frames = Vec::with_capacity(res.len());
    let mut error = None;

    for r in res {
        match r {
            parser::Response::Success { fields, binary } => {
                let values = fields
                    .iter()
                    .map(|(k, v)| (String::from(*k), String::from(*v)))
                    .collect();

                let binary = binary.map(|bin| Bytes::copy_from_slice(bin));

                frames.push(Frame { values, binary });
            }
            parser::Response::Error {
                code,
                command_index,
                current_command,
                message,
            } => {
                assert!(
                    error.is_none(),
                    "response contained more than a single error"
                );

                error = Some(ResponseError {
                    code: *code,
                    command_index: *command_index,
                    current_command: current_command.map(String::from),
                    message: String::from(*message),
                });
            }
        }
    }

    Response::new(frames, error)
}

/// Errors which can occur during operation
#[derive(Debug)]
pub enum MpdCodecError {
    /// IO error occured
    Io(io::Error),
    /// Did not get expected greeting as first message (`OK MPD <protocol version>`)
    InvalidGreeting(Bytes),
    /// A message could not be parsed succesfully.
    InvalidResponse(Bytes),
    /// A command string passed to the encoder was invalid (empty or contained a newline)
    InvalidCommand(String),
}

impl fmt::Display for MpdCodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MpdCodecError::Io(e) => write!(f, "{}", e),
            MpdCodecError::InvalidGreeting(greeting) => {
                write!(f, "invalid greeting: {:?}", greeting)
            }
            MpdCodecError::InvalidResponse(response) => {
                write!(f, "invalid response: {:?}", response)
            }
            MpdCodecError::InvalidCommand(command) => write!(f, "invalid command: {:?}", command),
        }
    }
}

impl From<io::Error> for MpdCodecError {
    fn from(e: io::Error) -> Self {
        MpdCodecError::Io(e)
    }
}

impl Error for MpdCodecError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            MpdCodecError::Io(e) => Some(e),
            _ => None,
        }
    }
}
