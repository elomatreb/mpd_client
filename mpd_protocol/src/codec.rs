//! [Codec] for MPD protocol.
//!
//! The codec accepts sending arbitrary (single) messages, it is up to you to make sure they are
//! valid.
//!
//! See the notes on the [`parser`] module about what responses the codec
//! supports.
//!
//! [Codec]: https://docs.rs/tokio-util/0.2.0/tokio_util/codec/index.html
//! [`parser`]: ../parser/index.html

use bytes::{Buf, Bytes, BytesMut};
use log::{debug, info, trace};
use tokio_util::codec::{Decoder, Encoder};

use std::error::Error;
use std::fmt;
use std::io;

use crate::command::CommandList;
use crate::parser;
use crate::response::{Error as ResponseError, Frame, Response};

/// [Codec] for MPD protocol.
///
/// The `Encoder` implemention consumes [`CommandList`]s, but single commands can trivially be
/// converted into lists and won't needlessly be wrapped.
///
/// [Codec]: https://docs.rs/tokio-util/0.2.0/tokio_util/codec/index.html
/// [`CommandList`]: ../command/struct.CommandList.html
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct MpdCodec {
    cursor: usize,
    protocol_version: Option<String>,
}

impl MpdCodec {
    /// Creates a new `MpdCodec`.
    pub fn new() -> Self {
        MpdCodec::default()
    }

    /// Returns the protocol version the server is speaking if this decoder instance already
    /// received a greeting, `None` otherwise.
    pub fn protocol_version(&self) -> Option<&str> {
        self.protocol_version.as_ref().map(String::as_str)
    }
}

impl Encoder<CommandList> for MpdCodec {
    type Error = MpdCodecError;

    fn encode(&mut self, command: CommandList, buf: &mut BytesMut) -> Result<(), Self::Error> {
        trace!("encode: Command {:?}", command);

        buf.extend_from_slice(command.render().as_bytes());

        Ok(())
    }
}

impl Decoder for MpdCodec {
    type Item = Response;
    type Error = MpdCodecError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        trace!("decode: {} bytes of buffer", src.len());

        if self.protocol_version.is_none() {
            match parser::greeting(src) {
                Ok((rem, greeting)) => {
                    info!("decode: Greeted by server, version {:?}", greeting.version);

                    self.protocol_version = Some(greeting.version.to_owned());

                    // Drop the part of the buffer containing the greeting
                    let new_start = src.len() - rem.len();
                    src.advance(new_start);
                    debug!(
                        "decode: Dropping {} bytes of greeting, remaining length: {}",
                        new_start,
                        src.len()
                    );
                }
                Err(e) => {
                    if e.is_incomplete() {
                        trace!("decode: Greeting incomplete");
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
            trace!("decode: Message potentially ends at index {}", msg_end);

            match parser::response(&src[..msg_end]) {
                Ok((_remainder, response)) => {
                    let r = convert_raw_response(&response);

                    src.advance(msg_end);
                    self.cursor = 0;

                    debug!(
                        "decoder: Response complete, {} bytes of buffer remaining",
                        src.len()
                    );
                    return Ok(Some(r));
                }
                Err(e) => {
                    if !e.is_incomplete() {
                        return Err(MpdCodecError::InvalidResponse(src.split().freeze()));
                    }
                    trace!("decode: Message incomplete");
                }
            }
        }

        // We didn't find a terminator

        // Subtract two in case the terminator was already partially in the buffer
        self.cursor = src.len().saturating_sub(2);
        trace!(
            "decode: Starting next search for message terminator at index {}",
            self.cursor
        );

        Ok(None)
    }
}

/// Convert the raw parsed response to one with owned data
fn convert_raw_response(res: &[parser::Response<'_>]) -> Response {
    let mut frames = Vec::with_capacity(res.len());
    let mut error = None;

    for r in res {
        match r {
            parser::Response::Success { fields, binary } => {
                let values = fields
                    .iter()
                    .map(|(k, v)| (String::from(*k), String::from(*v)))
                    .collect();

                let binary = binary.map(|bin| Vec::from(bin));

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

/// Errors which can occur during [`MpdCodec`] operation.
///
/// [`MpdCodec`]: struct.MpdCodec.html
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
