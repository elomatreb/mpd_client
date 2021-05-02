//! Basic facilities for using the protocol using synchronous IO.

use bytes::BytesMut;
use tracing::{debug, error, span, trace, Level};

use std::io::{self, BufRead, Write};

use crate::{parser, response::ResponseBuilder, Command, CommandList, MpdProtocolError, Response};

/// Connect to a server using the given IO.
///
/// This reads the handshake from the server, and returns the protocol version if sucessful.
///
/// # Errors
///
/// This will error if an IO error occurs, or if the server sends an invalid greeting message.
pub fn connect<IO>(mut io: IO) -> Result<Box<str>, MpdProtocolError>
where
    IO: BufRead,
{
    let span = span!(Level::DEBUG, "connect");
    let _enter = span.enter();

    let mut greeting = Vec::new();
    io.read_until(b'\n', &mut greeting)?;

    match parser::greeting(&greeting) {
        Ok((_, version)) => {
            debug!(?version, "connected");
            Ok(Box::from(version))
        }
        Err(_) => Err(MpdProtocolError::InvalidMessage),
    }
}

/// Read a complete response from the given IO.
///
/// This will return `Some` with a [`Response`] if a complete response could be read, or `None` if
/// the stream closed.
///
/// # Errors
///
/// This will return an error if reading from the IO returns an error, if EOF is encountered while
/// in the middle of a response, or if the server sends an invalid response.
pub fn receive<IO>(mut io: IO) -> Result<Option<Response>, MpdProtocolError>
where
    IO: BufRead,
{
    let span = span!(Level::DEBUG, "receive");
    let _enter = span.enter();

    let mut src = BytesMut::new();
    let mut response = ResponseBuilder::new();

    loop {
        let read = read_until(&mut io, b'\n', &mut src)?;

        if let Some(resp) = response.parse(&mut src)? {
            debug!("parsed complete response");
            break Ok(Some(resp));
        } else if read == 0 {
            // Reached EOF
            if response.is_frame_in_progress() || !src.is_empty() {
                error!("EOF while reading frame");
                break Err(MpdProtocolError::Io(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "unexpected end of response",
                )));
            } else {
                debug!("EOF while no frame in progress");
                break Ok(None);
            }
        } else {
            trace!("response incomplete");
        }
    }
}

/// Read all bytes into `buf` until the delimiter `byte` or EOF is reached.
fn read_until<R: BufRead>(r: &mut R, delim: u8, buf: &mut BytesMut) -> Result<usize, io::Error> {
    // Adapted from implementation of standard library `BufRead::read_until`
    let mut read = 0;
    loop {
        let (done, used) = {
            let available = match r.fill_buf() {
                Ok(n) => n,
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            };
            match memchr::memchr(delim, available) {
                Some(i) => {
                    buf.extend_from_slice(&available[..=i]);
                    (true, i + 1)
                }
                None => {
                    buf.extend_from_slice(available);
                    (false, available.len())
                }
            }
        };
        r.consume(used);
        read += used;
        if done || used == 0 {
            return Ok(read);
        }
    }
}

/// Send the given [`Command`] using the given IO.
///
/// # Errors
///
/// This will return an error if writing to the IO returns an error.
pub fn send<IO>(io: IO, command: Command) -> Result<(), MpdProtocolError>
where
    IO: Write,
{
    send_list(io, CommandList::new(command))
}

/// Send the given [`CommandList`] using the given IO.
///
/// # Errors
///
/// This will return an error if writing to the IO returns an error.
pub fn send_list<IO>(mut io: IO, command_list: CommandList) -> Result<(), MpdProtocolError>
where
    IO: Write,
{
    let mut buf = BytesMut::new();
    command_list.render(&mut buf);

    io.write_all(&buf)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use std::io::Cursor;

    #[test]
    fn read() {
        let mut buf = Cursor::new("OK MPD 0.22.0\nOK\nfoo: bar\nOK\n");

        let version = connect(&mut buf).unwrap();
        assert_eq!(&*version, "0.22.0");
        assert_eq!(buf.position(), 14);

        let resp = receive(&mut buf).unwrap();
        assert_eq!(resp, Some(Response::empty()));
        assert_eq!(buf.position(), 17);

        let resp = receive(&mut buf)
            .expect("receive error")
            .expect("no response");
        let frame = resp.single_frame().unwrap();
        assert_eq!(frame.find("foo"), Some("bar"));
        assert_eq!(&buf.get_ref()[buf.position() as usize..], "");

        // EOF
        assert_eq!(receive(&mut buf).unwrap(), None);
    }

    #[test]
    fn write() {
        const GREETING: &[u8] = b"OK MPD 0.22.0\n";
        let mut io = Cursor::new(Vec::from(GREETING));

        connect(&mut io).unwrap();

        send(&mut io, Command::new("playid").argument("3")).unwrap();

        assert_eq!(&io.get_ref()[GREETING.len()..], b"playid 3\n");
    }

    #[test]
    fn eof() {
        let buf = "".as_bytes();
        assert_matches!(receive(buf), Ok(None));

        let buf = "foo: bar\n".as_bytes();
        assert_matches!(receive(buf), Err(MpdProtocolError::Io(_)));

        let buf = "foo: bar".as_bytes();
        assert_matches!(receive(buf), Err(MpdProtocolError::Io(_)));
    }
}
