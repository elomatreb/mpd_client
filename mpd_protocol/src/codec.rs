//! [Codec] for MPD protocol.
//!
//! The codec accepts sending arbitrary (single) messages, it is up to you to make sure they are
//! valid.
//!
//! See the notes on the [`parser`] module about what responses the codec
//! supports.
//!
//! [Codec]: https://docs.rs/tokio-util/0.3.0/tokio_util/codec/index.html
//! [`parser`]: crate::parser

use bytes::{Buf, BytesMut};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite};
use tokio_util::codec::{Decoder, Encoder, Framed};
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
/// [Codec]: https://docs.rs/tokio-util/0.3.0/tokio_util/codec/index.html
/// [`CommandList`]: crate::command::CommandList
#[derive(Clone, Debug)]
pub struct MpdCodec {
    decode_span: Option<Span>,
    cursor: usize,
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
    pub async fn connect<IO>(mut io: IO) -> Result<Framed<IO, Self>, ConnectError>
    where
        IO: AsyncRead + AsyncWrite + Unpin,
    {
        let mut greeting = [0u8; 32];
        let mut read = 0;

        loop {
            read += io.read(&mut greeting).await?;

            match parser::greeting(&greeting[..read]) {
                Ok((_, parser::Greeting { version })) => {
                    info!(protocol_version = version, "connected successfully");

                    let codec = Self {
                        decode_span: None,
                        cursor: 0,
                        protocol_version: version.into(),
                    };

                    break Ok(Framed::new(io, codec));
                }
                Err(e) => {
                    if !e.is_incomplete() || read == greeting.len() - 1 {
                        break Err(ConnectError::InvalidGreeting(greeting[..read].into()));
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

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if self.decode_span.is_none() {
            self.decode_span = Some(span!(Level::DEBUG, "decode_command"));
        }

        let enter = self.decode_span.as_ref().unwrap().enter();

        trace!(self.cursor);

        for (terminator, _) in src[self.cursor..]
            .windows(3)
            .enumerate()
            .filter(|(_, w)| w == b"OK\n")
        {
            let msg_end = self.cursor + terminator + 3;
            trace!(end = msg_end, "potential response end");

            let parser_result = parser::response(&src[..msg_end]);
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
                        return Err(MpdCodecError::InvalidResponse(err.as_ref().into()));
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

/// Errors which can occur when initially connecting an [`MpdCodec`].
#[derive(Debug)]
pub enum ConnectError {
    /// IO error
    Io(io::Error),
    /// Invalid greeting message
    InvalidGreeting(Box<[u8]>),
}

impl fmt::Display for ConnectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(_) => write!(f, "IO error"),
            Self::InvalidGreeting(_) => write!(f, "Invalid greeting"),
        }
    }
}

#[doc(hidden)]
impl From<io::Error> for ConnectError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl Error for ConnectError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

/// Errors which can occur during [`MpdCodec`] operation.
#[derive(Debug)]
pub enum MpdCodecError {
    /// IO error occured
    Io(io::Error),
    /// A message could not be parsed succesfully.
    InvalidResponse(Box<[u8]>),
}

impl fmt::Display for MpdCodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MpdCodecError::Io(_) => write!(f, "IO error"),
            MpdCodecError::InvalidResponse(_) => write!(f, "Invalid response"),
        }
    }
}

#[doc(hidden)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn dummy_codec() -> MpdCodec {
        MpdCodec {
            decode_span: None,
            cursor: 0,
            protocol_version: "".into(),
        }
    }

    fn init_buffer(msg: &[u8]) -> BytesMut {
        BytesMut::from(msg)
    }

    #[test]
    fn encoder() {
        let mut codec = dummy_codec();
        let buf = &mut BytesMut::new();

        let command = CommandList::new(Command::build("status").unwrap());

        codec.encode(command.clone(), buf).unwrap();

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

    #[test]
    fn empty_response() {
        let mut codec = dummy_codec();
        let buf = &mut init_buffer(b"OK");

        assert_eq!(None, codec.decode(buf).unwrap());

        buf.extend_from_slice(b"\n");

        assert_eq!(Some(Response::empty()), codec.decode(buf).unwrap());
    }

    #[test]
    fn simple_response() {
        let mut codec = dummy_codec();
        let buf = &mut init_buffer(b"hello: world\nfoo: OK\nbar: 1234\nOK");

        assert_eq!(None, codec.decode(buf).unwrap());

        buf.extend_from_slice(b"\n");

        let response = codec.decode(buf).expect("failed to decode").unwrap();
        let frame = response.single_frame().unwrap();

        assert_eq!(frame.find("hello"), Some("world"));
        assert_eq!(frame.find("foo"), Some("OK"));
        assert_eq!(frame.find("bar"), Some("1234"));

        assert!(buf.is_empty());
    }

    #[test]
    fn decoder_ignores_trailing_data() {
        let mut codec = dummy_codec();
        let buf = &mut init_buffer(b"foo: OK\nOK\ntrailing_stuff");

        let _ = codec.decode(buf).unwrap();

        assert_eq!(buf, "trailing_stuff");
    }

    #[test]
    fn command_list() {
        let mut codec = dummy_codec();
        let buf = &mut init_buffer(b"list_OK\nfoo: bar\nlist_OK\nbinary: 6\nBINARY\nlist_OK\nOK");

        assert_eq!(None, codec.decode(buf).unwrap());

        buf.extend_from_slice(b"\n");

        let mut response = codec
            .decode(buf)
            .expect("failed to decode")
            .unwrap()
            .into_frames();

        let first = response.next().unwrap().unwrap();
        let second = response.next().unwrap().unwrap();
        let mut third = response.next().unwrap().unwrap();

        assert!(buf.is_empty());

        assert!(first.is_empty());

        assert_eq!(second.find("foo"), Some("bar"));

        assert_eq!(third.find("binary"), None);
        assert_eq!(third.get_binary(), Some(Vec::from("BINARY")));
    }

    #[test]
    fn binary_response() {
        let mut codec = dummy_codec();
        let buf = &mut init_buffer(b"binary: 16\nHELLO \nOK\n");

        assert_eq!(None, codec.decode(buf).unwrap());

        buf.extend_from_slice(b" WORLD\nOK\n");

        let response = codec.decode(buf).expect("failed to decode").unwrap();
        let mut frame = response.single_frame().unwrap();

        assert_eq!(frame.fields_len(), 0);
        assert_eq!(frame.get_binary(), Some(Vec::from("HELLO \nOK\n WORLD")));

        assert!(buf.is_empty());
    }

    #[test]
    fn multiple_messages() {
        let mut codec = dummy_codec();
        let buf = &mut init_buffer(b"foo: bar\nOK\nhello: world\nOK\n");

        let response = codec.decode(buf).expect("failed to decode").unwrap();
        let frame = response.single_frame().unwrap();

        assert_eq!(frame.find("foo"), Some("bar"));
        assert_eq!(&buf[..], b"hello: world\nOK\n");

        let response = codec.decode(buf).expect("failed to decode").unwrap();
        let frame = response.single_frame().unwrap();

        assert_eq!(frame.find("hello"), Some("world"));
        assert!(buf.is_empty());
    }

    #[test]
    fn cursor_reset() {
        let mut codec = dummy_codec();
        let buf = &mut init_buffer(b"hello: world\nOK");

        assert_eq!(None, codec.decode(buf).unwrap());

        buf.extend_from_slice(b"\na: b\nOK\n");

        assert!(codec.decode(buf).unwrap().is_some());
        assert!(codec.decode(buf).unwrap().is_some());
    }
}
