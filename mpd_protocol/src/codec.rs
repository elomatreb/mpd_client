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

use bytes::{Buf, BytesMut};
use tokio_util::codec::{Decoder, Encoder};
use tracing::{debug, error, info, span, trace, Level, Span};

use std::convert::TryFrom;
use std::error::Error;
use std::fmt;
use std::io;

use crate::command::{Command, CommandList};
use crate::parser;
use crate::response::Response;

/// [Codec] for MPD protocol.
///
/// The `Encoder` implemention consumes [`CommandList`]s, but single commands can trivially be
/// converted into lists and won't needlessly be wrapped.
///
/// [Codec]: https://docs.rs/tokio-util/0.2.0/tokio_util/codec/index.html
/// [`CommandList`]: ../command/struct.CommandList.html
#[derive(Clone, Debug, Default)]
pub struct MpdCodec {
    decode_span: Option<Span>,
    cursor: usize,
    protocol_version: Option<String>,
}

impl MpdCodec {
    /// Creates a new `MpdCodec`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the protocol version the server is speaking if this decoder instance already
    /// received a greeting, `None` otherwise.
    pub fn protocol_version(&self) -> Option<&str> {
        self.protocol_version.as_deref()
    }
}

impl Encoder<Command> for MpdCodec {
    type Error = MpdCodecError;

    fn encode(&mut self, command: Command, dst: &mut BytesMut) -> Result<(), Self::Error> {
        // This is free since CommandList stores its first item inline
        let command_list = CommandList::new(command);
        self.encode(command_list, dst)
    }
}

impl Encoder<CommandList> for MpdCodec {
    type Error = MpdCodecError;

    fn encode(&mut self, command: CommandList, buf: &mut BytesMut) -> Result<(), Self::Error> {
        let span = span!(Level::DEBUG, "encode_command", ?command);
        let _enter = span.enter();

        let len_before = buf.len();
        command.render(buf);
        trace!(encoded_length = buf.len() - len_before);

        Ok(())
    }
}

impl Decoder for MpdCodec {
    type Item = Response;
    type Error = MpdCodecError;

    #[allow(clippy::cognitive_complexity)]
    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if self.decode_span.is_none() {
            self.decode_span = Some(span!(Level::DEBUG, "decode_command"));
        }

        let enter = self.decode_span.as_ref().unwrap().enter();

        trace!(
            buffer_length = src.len(),
            greeted = self.protocol_version.is_some()
        );

        if self.protocol_version.is_none() {
            match parser::greeting(src) {
                Ok((rem, greeting)) => {
                    info!(protocol_version = greeting.version);
                    self.protocol_version = Some(greeting.version.to_owned());

                    // Drop the part of the buffer containing the greeting
                    let new_start = src.len() - rem.len();
                    src.advance(new_start);
                    trace!(buffer_after_greeting = src.len());
                }
                Err(e) => {
                    if e.is_incomplete() {
                        trace!("greeting incomplete");
                        return Ok(None);
                    } else {
                        // We got a malformed greeting
                        error!(error = ?e, "error parsing greeting");
                        let err = src.split();
                        self.cursor = 0;
                        return Err(MpdCodecError::InvalidGreeting(Vec::from(&err[..])));
                    }
                }
            }
        }

        trace!(self.cursor);

        for (terminator, _) in src[self.cursor..]
            .windows(3)
            .enumerate()
            .filter(|(_, w)| w == b"OK\n")
        {
            let msg_end = self.cursor + terminator + 3;
            trace!(end = msg_end, "potential response end");

            let parser_result = parser::response(&src[..]);
            trace!("completed parsing");

            match parser_result {
                Ok((_remainder, response)) => {
                    // The errors returned by the TryFrom impl are not possible when operating
                    // directly on the results of our parser
                    let r = Response::try_from(response.as_slice()).unwrap();

                    src.advance(msg_end);

                    debug!(
                        response = ?r,
                        remaining_buffer = src.len(),
                        "response complete",
                    );

                    drop(enter);
                    self.cursor = 0;
                    self.decode_span = None;

                    return Ok(Some(r));
                }
                Err(e) => {
                    if !e.is_incomplete() {
                        error!(error = ?e, "error parsing response");
                        let err = src.split();
                        self.cursor = 0;
                        return Err(MpdCodecError::InvalidResponse(Vec::from(&err[..])));
                    } else {
                        trace!("response incomplete");
                    }
                }
            }
        }

        // We didn't find a terminator or the message was incomplete

        // Subtract two in case the terminator was already partially in the buffer
        self.cursor = src.len().saturating_sub(2);

        Ok(None)
    }
}

/// Errors which can occur during [`MpdCodec`] operation.
///
/// [`MpdCodec`]: struct.MpdCodec.html
#[derive(Debug)]
pub enum MpdCodecError {
    /// IO error occured
    Io(io::Error),
    /// Did not get expected greeting as first message (`OK MPD <protocol version>`)
    InvalidGreeting(Vec<u8>),
    /// A message could not be parsed succesfully.
    InvalidResponse(Vec<u8>),
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
