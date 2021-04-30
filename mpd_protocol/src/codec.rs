//! [Codec] for MPD protocol.
//!
//! The codec accepts sending arbitrary (single) messages, it is up to you to make sure they are
//! valid.
//!
//! [Codec]: https://docs.rs/tokio-util/0.6.0/tokio_util/codec/index.html

use bytes::BytesMut;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite};
use tokio_util::codec::{Decoder, Encoder, Framed};
use tracing::{debug, error, info, span, Level, Span};

use std::error::Error as StdError;
use std::fmt;
use std::io;

use crate::command::{Command, CommandList};
use crate::parser;
use crate::response::{Response, ResponseBuilder};

/// [Codec] for MPD protocol.
///
/// [Codec]: https://docs.rs/tokio-util/0.6.0/tokio_util/codec/index.html
/// [`CommandList`]: crate::command::CommandList
#[derive(Clone, Debug)]
pub struct MpdCodec {
    log_span: Span,
    current_response: ResponseBuilder,
    protocol_version: Box<str>,
}

impl MpdCodec {
    /// Connect using the given IO object.
    ///
    /// This reads the initial handshake from the server that contains the protocol version, which
    /// is then available using the [`MpdCodec::protocol_version()`] method.
    ///
    /// # Errors
    ///
    /// This returns an error when reading from the given IO object returns an error, or if the
    /// data read from it fails to parse as a valid server handshake.
    pub async fn connect<IO>(mut io: IO) -> Result<Framed<IO, Self>, MpdCodecError>
    where
        IO: AsyncRead + AsyncWrite + Unpin,
    {
        let mut greeting = [0u8; 32];
        let mut read = 0;

        loop {
            read += io.read(&mut greeting).await?;

            match parser::greeting(&greeting[..read]) {
                Ok((_, version)) => {
                    let log_span = span!(Level::DEBUG, "codec", protocol_version = version);

                    let enter = log_span.enter();
                    info!("connected successfully");
                    drop(enter);

                    let codec = Self {
                        log_span,
                        current_response: ResponseBuilder::new(),
                        protocol_version: version.into(),
                    };

                    break Ok(Framed::new(io, codec));
                }
                Err(e) => {
                    if !e.is_incomplete() || read == greeting.len() - 1 {
                        error!("invalid greeting");
                        break Err(MpdCodecError::InvalidMessage);
                    }
                }
            }
        }
    }

    /// Returns the protocol version the server is speaking.
    pub fn protocol_version(&self) -> &str {
        &self.protocol_version
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
        let _enter = self.log_span.enter();
        debug!(?command, "encoded command");

        command.render(buf);

        Ok(())
    }
}

impl Decoder for MpdCodec {
    type Item = Response;
    type Error = MpdCodecError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let _enter = self.log_span.enter();
        self.current_response.parse(src)
    }
}

/// Errors which can occur during [`MpdCodec`] operation.
#[derive(Debug)]
pub enum MpdCodecError {
    /// IO error occured
    Io(io::Error),
    /// A message could not be parsed succesfully.
    InvalidMessage,
}

impl fmt::Display for MpdCodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MpdCodecError::Io(_) => write!(f, "IO error"),
            MpdCodecError::InvalidMessage => write!(f, "invalid message"),
        }
    }
}

#[doc(hidden)]
impl From<io::Error> for MpdCodecError {
    fn from(e: io::Error) -> Self {
        MpdCodecError::Io(e)
    }
}

impl StdError for MpdCodecError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            MpdCodecError::Io(e) => Some(e),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn encoder() {
        let mut codec = MpdCodec {
            log_span: Span::none(),
            current_response: ResponseBuilder::new(),
            protocol_version: "".into(),
        };
        let buf = &mut BytesMut::new();

        let command = CommandList::new(Command::build("status").unwrap());

        codec.encode(command, buf).unwrap();

        assert_eq!(&b"status\n"[..], buf);
    }

    #[tokio::test]
    async fn connect() {
        let mut buf = Vec::from(&b"OK MPD 0.21.11\n"[..]);
        let buf_len = buf.len() as u64;

        let codec = MpdCodec::connect(Cursor::new(&mut buf)).await.unwrap();

        assert_eq!(codec.get_ref().position(), buf_len);
        assert_eq!(codec.codec().protocol_version(), "0.21.11");

        let parts = codec.into_parts();

        assert!(parts.read_buf.is_empty());
        assert!(parts.write_buf.is_empty());
    }
}
