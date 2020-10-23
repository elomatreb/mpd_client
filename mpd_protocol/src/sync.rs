//! Basic facilities for using the protocol using synchronous IO.

use bytes::BytesMut;

use std::io::{self, BufRead, Write};

use crate::{
    parser::{self, ParsedComponent},
    response::{Error, ResponseBuilder},
    Command, CommandList, MpdCodecError, Response,
};

/// Connect to a server using the given IO.
///
/// This reads the handshake from the server, and returns the protocol version if sucessful.
///
/// # Errors
///
/// This will error if an IO error occurs, or if the server sends an invalid greeting message.
pub fn connect<IO>(mut io: IO) -> Result<Box<str>, MpdCodecError>
where
    IO: BufRead,
{
    let mut greeting = Vec::new();
    io.read_until(b'\n', &mut greeting)?;

    match parser::greeting(&greeting) {
        Ok((_, version)) => Ok(Box::from(version)),
        Err(_) => Err(MpdCodecError::InvalidMessage(greeting.into())),
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
pub fn receive<IO>(mut io: IO) -> Result<Option<Response>, MpdCodecError>
where
    IO: BufRead,
{
    let mut buf = Vec::new();
    let mut response = ResponseBuilder::new();
    let mut frame_in_progresss = false;

    loop {
        let read = io.read_until(b'\n', &mut buf)?;

        if read == 0 {
            // Reached EOF
            if frame_in_progresss {
                break Err(MpdCodecError::Io(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "unexpected end of response",
                )));
            } else {
                break Ok(None);
            }
        }

        let parsed = ParsedComponent::parse(&buf);

        if let Ok((_, p)) = &parsed {
            match p {
                ParsedComponent::EndOfResponse | ParsedComponent::Error(_) => {
                    frame_in_progresss = false
                }
                _ => frame_in_progresss = true,
            }
        }

        match parsed {
            Err(nom::Err::Incomplete(_)) => continue,
            Err(_) => break Err(MpdCodecError::InvalidMessage(buf.into())),
            Ok((_, parsed)) => match parsed {
                ParsedComponent::Field { key, value } => response.push_field(key, value.to_owned()),
                ParsedComponent::BinaryField(bin) => response.push_binary(BytesMut::from(bin)),
                ParsedComponent::EndOfFrame => response.finish_frame(),
                ParsedComponent::EndOfResponse => {
                    break Ok(Some(response.finish()));
                }
                ParsedComponent::Error(e) => {
                    let error = Error::from_parsed(&e);
                    break Ok(Some(response.error(error)));
                }
            },
        }

        buf.clear();
    }
}

/// Send the given [`Command`] using the given IO.
///
/// # Errors
///
/// This will return an error if writing to the IO returns an error.
pub fn send<IO>(io: IO, command: Command) -> Result<(), MpdCodecError>
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
pub fn send_list<IO>(mut io: IO, command_list: CommandList) -> Result<(), MpdCodecError>
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
}
