//! [Codec] for MPD protocol.
//!
//! [Codec]: https://docs.rs/tokio-util/0.6.6/tokio_util/codec/index.html

use bytes::BytesMut;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite};
use tokio_util::codec::{Decoder, Encoder, Framed};
use tracing::{debug, error, info, span, Level, Span};

use std::io;

use crate::command::{Command, CommandList};
use crate::parser;
use crate::response::{Response, ResponseBuilder};
use crate::MpdProtocolError;

/// [Codec] for MPD protocol.
///
/// [Codec]: https://docs.rs/tokio-util/0.6.6/tokio_util/codec/index.html
#[derive(Clone, Debug)]
#[cfg_attr(docsrs, doc(cfg(feature = "async")))]
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
    pub async fn connect<IO>(mut io: IO) -> Result<Framed<IO, Self>, MpdProtocolError>
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
                        break Err(MpdProtocolError::InvalidMessage);
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
    type Error = MpdProtocolError;

    fn encode(&mut self, command: Command, dst: &mut BytesMut) -> Result<(), Self::Error> {
        // This is free since CommandList stores its first item inline
        let command_list = CommandList::new(command);
        self.encode(command_list, dst)
    }
}

impl Encoder<CommandList> for MpdCodec {
    type Error = MpdProtocolError;

    fn encode(&mut self, command: CommandList, buf: &mut BytesMut) -> Result<(), Self::Error> {
        let _enter = self.log_span.enter();
        debug!(?command, "encoded command");

        command.render(buf);

        Ok(())
    }
}

impl Decoder for MpdCodec {
    type Item = Response;
    type Error = MpdProtocolError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let _enter = self.log_span.enter();
        self.current_response.parse(src)
    }

    fn decode_eof(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let _enter = self.log_span.enter();

        if !buf.is_empty() || self.current_response.is_frame_in_progress() {
            error!("EOF while frame in progress");
            Err(MpdProtocolError::Io(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "unexpected end of response",
            )))
        } else {
            debug!("EOF while no frame in progress");
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use futures::{sink::SinkExt, stream::StreamExt};
    use std::io::Cursor;
    use tokio_test::io::Builder as MockBuilder;

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

    #[tokio::test]
    async fn full_interaction() {
        let io = MockBuilder::new()
            .read(b"OK MPD 0.21.11\n")
            .write(b"status\n")
            .read(b"foo: bar\nOK\n")
            .build();

        let mut conn = MpdCodec::connect(io).await.unwrap();
        assert_eq!(conn.codec().protocol_version(), "0.21.11");

        conn.send(Command::new("status")).await.unwrap();

        let response = conn.next().await.unwrap().unwrap();
        assert_eq!(response.successful_frames(), 1);

        let frame = response.single_frame().unwrap();
        assert_eq!(frame.find("foo"), Some("bar"));
    }

    #[tokio::test]
    async fn eof() {
        let io = MockBuilder::new().read(b"OK MPD 0.21.11\n").build();
        let mut conn = MpdCodec::connect(io).await.unwrap();
        assert_matches!(conn.next().await, None);

        // Incomplete frame
        let io = MockBuilder::new()
            .read(b"OK MPD 0.21.11\n")
            .read(b"foo: bar\n")
            .build();
        let mut conn = MpdCodec::connect(io).await.unwrap();
        assert_matches!(conn.next().await, Some(Err(MpdProtocolError::Io(_))));

        // Incomplete frame with unconsumed data
        let io = MockBuilder::new()
            .read(b"OK MPD 0.21.11\n")
            .read(b"foo: bar")
            .build();
        let mut conn = MpdCodec::connect(io).await.unwrap();
        assert_matches!(conn.next().await, Some(Err(MpdProtocolError::Io(_))));
    }
}
