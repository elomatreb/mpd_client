//! [Codec] for MPD protocol.
//!
//! The codec accepts sending arbitrary (single) messages, it is up to you to make sure they are
//! valid.
//!
//! [Codec]: https://docs.rs/tokio-util/0.3.0/tokio_util/codec/index.html

use bytes::{Buf, BytesMut};
use nom::{Err as NomErr, Needed};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite};
use tokio_util::codec::{Decoder, Encoder, Framed};
use tracing::{debug, error, info, span, trace, Level, Span};

use std::error::Error as StdError;
use std::fmt;
use std::io;
use std::mem;

use crate::command::{Command, CommandList};
use crate::parser::{self, ParsedComponent};
use crate::response::{error::Error, Response, ResponseBuilder};

/// [Codec] for MPD protocol.
///
/// [Codec]: https://docs.rs/tokio-util/0.3.0/tokio_util/codec/index.html
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
                        break Err(MpdCodecError::InvalidMessage(greeting[..read].into()));
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

        loop {
            let parsed = ParsedComponent::parse(&src);

            match parsed {
                Err(NomErr::Incomplete(needed)) => {
                    if let Needed::Size(n) = needed {
                        src.reserve(n.get().saturating_sub(src.len()));
                    }

                    break Ok(None);
                }
                Err(_) => {
                    error!("invalid message");
                    break Err(MpdCodecError::InvalidMessage(src[..].into()));
                }
                Ok((remaining, parsed_item)) => {
                    let mut ret = None;
                    let msg_end = src.len() - remaining.len();

                    match parsed_item {
                        ParsedComponent::Field { key, value } => {
                            self.current_response.push_field(key, value.to_owned())
                        }
                        ParsedComponent::BinaryField(bin) => {
                            let bin_start = bin.as_ptr() as usize;
                            let bin_len = bin.len();

                            let mut binary = src.split_to(msg_end);
                            let prefix_len = bin_start - binary.as_ptr() as usize;

                            binary.advance(prefix_len);
                            binary.truncate(bin_len);

                            trace!(length = binary.len(), "pushing binary object");
                            self.current_response.push_binary(binary);
                            continue;
                        }
                        ParsedComponent::EndOfFrame => self.current_response.finish_frame(),
                        ParsedComponent::EndOfResponse => {
                            let response = mem::take(&mut self.current_response).finish();
                            trace!(frames = response.len(), "message decoded successfully");
                            ret = Some(response);
                        }
                        ParsedComponent::Error(e) => {
                            let response_builder = mem::take(&mut self.current_response);
                            let response = response_builder.error(Error::from_parsed(&e));
                            trace!(
                                error = ?e,
                                successful_frames = response.len(),
                                "message decoded with protocol error"
                            );
                            ret = Some(response);
                        }
                    }

                    src.advance(msg_end);

                    if let Some(response) = ret {
                        break Ok(Some(response));
                    }
                }
            }
        }
    }
}

/// Errors which can occur during [`MpdCodec`] operation.
#[derive(Debug)]
pub enum MpdCodecError {
    /// IO error occured
    Io(io::Error),
    /// A message could not be parsed succesfully.
    InvalidMessage(Box<[u8]>),
}

impl fmt::Display for MpdCodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MpdCodecError::Io(_) => write!(f, "IO error"),
            MpdCodecError::InvalidMessage(_) => write!(f, "invalid message"),
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

    fn dummy_codec() -> MpdCodec {
        MpdCodec {
            log_span: Span::none(),
            current_response: ResponseBuilder::new(),
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
            .into_iter();

        let first = response.next().unwrap().unwrap();
        let second = response.next().unwrap().unwrap();
        let mut third = response.next().unwrap().unwrap();

        assert!(buf.is_empty());

        assert!(first.is_empty());

        assert_eq!(second.find("foo"), Some("bar"));

        assert_eq!(third.find("binary"), None);
        assert_eq!(third.get_binary(), Some(BytesMut::from("BINARY")));
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
        assert_eq!(
            frame.get_binary(),
            Some(BytesMut::from("HELLO \nOK\n WORLD"))
        );

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
