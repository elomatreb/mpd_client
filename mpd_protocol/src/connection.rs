use std::io::{self, Read, Write};

use bytes::{BufMut, BytesMut};
#[cfg(feature = "async")]
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tracing::{debug, error, info, trace};

use crate::{
    command::{Command, CommandList},
    parser,
    response::{Response, ResponseBuilder, ResponseFieldCache},
    MpdProtocolError,
};

/// Default receive buffer size
const DEFAULT_BUFFER_CAPACITY: usize = 4096;

/// A **blocking** connection to an MPD server.
#[derive(Debug)]
pub struct Connection<IO> {
    io: IO,
    protocol_version: Box<str>,
    field_cache: ResponseFieldCache,
    recv_buf: BytesMut,
    total_received: usize,
}

impl<IO> Connection<IO> {
    #[cfg(any(fuzzing, criterion))]
    #[allow(dead_code)]
    #[doc(hidden)]
    pub fn new_internal(io: IO) -> Connection<IO> {
        Connection {
            io,
            protocol_version: Box::from(""),
            field_cache: ResponseFieldCache::new(),
            recv_buf: BytesMut::zeroed(DEFAULT_BUFFER_CAPACITY),
            total_received: 0,
        }
    }

    /// Connect to an MPD server synchronously.
    #[tracing::instrument(skip_all, err)]
    pub fn connect(mut io: IO) -> Result<Connection<IO>, MpdProtocolError>
    where
        IO: Read,
    {
        let mut recv_buf = BytesMut::zeroed(DEFAULT_BUFFER_CAPACITY);
        let mut total_read = 0;

        let protocol_version = loop {
            let (data, amount_read) = read_to_buffer(&mut io, &mut recv_buf, &mut total_read)?;

            if amount_read == 0 {
                return Err(MpdProtocolError::Io(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "unexpected end of file while receiving greeting",
                )));
            }

            match parser::greeting(data) {
                Ok((_, version)) => {
                    info!(?version, "connected successfully");
                    break Box::from(version);
                }
                Err(e) if e.is_incomplete() => {
                    // The response was valid *so far*, try another read
                    trace!("greeting incomplete");
                }
                Err(_) => {
                    error!("invalid greeting");
                    return Err(MpdProtocolError::InvalidMessage);
                }
            }
        };

        Ok(Connection {
            io,
            protocol_version,
            field_cache: ResponseFieldCache::new(),
            recv_buf,
            total_received: 0,
        })
    }

    /// Send a command.
    ///
    /// # Errors
    ///
    /// This will return an error if writing to the given IO resource fails.
    #[tracing::instrument(skip(self), err)]
    pub fn send(&mut self, mut command: Command) -> Result<(), MpdProtocolError>
    where
        IO: Write,
    {
        command.0.put_u8(b'\n');
        self.io.write_all(&command.0)?;
        debug!(length = command.0.len(), "sent command");
        Ok(())
    }

    /// Send a command list.
    ///
    /// # Errors
    ///
    /// This will return an error if writing to the given IO resource fails.
    #[tracing::instrument(skip(self), err)]
    pub fn send_list(&mut self, command_list: CommandList) -> Result<(), MpdProtocolError>
    where
        IO: Write,
    {
        let buf = command_list.render();
        self.io.write_all(&buf)?;
        debug!(length = buf.len(), "sent command list");

        Ok(())
    }

    /// Receive a response from the server.
    ///
    /// This will return `Ok(Some(..))` when a complete response has been received, or `Ok(None)` if
    /// the connection is closed cleanly.
    ///
    /// # Errors
    ///
    /// This will return an error if:
    ///
    ///  - Reading from the given IO resource returns an error
    ///  - Malformed response data is received
    ///  - The connection is closed while a response is in progress
    #[tracing::instrument(skip(self), err)]
    pub fn receive(&mut self) -> Result<Option<Response>, MpdProtocolError>
    where
        IO: Read,
    {
        let mut response_builder = ResponseBuilder::new(&mut self.field_cache);

        loop {
            // Split off the read part of the receive buffer
            let buf_size = self.recv_buf.len();
            let remaining = self.recv_buf.split_off(self.total_received);

            // Try to parse response data from the initialized section of the buffer, removing the
            // consumed parts from the buffer
            let maybe_parsed = response_builder.parse(&mut self.recv_buf)?;

            // Update the length of the initialized section to the remaining length
            self.total_received = self.recv_buf.len();

            // Join back the remaining data with the main buffer, and readjust the length
            self.recv_buf.unsplit(remaining);
            self.recv_buf.resize(buf_size, 0);

            if let Some(response) = maybe_parsed {
                debug!(
                    frames = response.successful_frames(),
                    error = response.is_error(),
                    fields = response.field_count(),
                    "received complete response"
                );
                break Ok(Some(response));
            }

            let (_, amount_read) =
                read_to_buffer(&mut self.io, &mut self.recv_buf, &mut self.total_received)?;

            if amount_read == 0 {
                break if response_builder.is_frame_in_progress() || self.total_received != 0 {
                    error!("EOF while receiving response");
                    Err(MpdProtocolError::Io(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "unexpected end of file while receiving response",
                    )))
                } else {
                    debug!("clean EOF while receiving response");
                    Ok(None)
                };
            }
        }
    }

    /// Send a command and receive its response.
    ///
    /// This is essentially a shorthand for [`Connection::send`] followed by [`Connection::receive`].
    ///
    /// # Errors
    ///
    /// This will return an error if:
    ///
    ///  - Writing to or reading from the connection returns an error
    ///  - Malformed response data is received
    ///  - The connection is closed
    #[tracing::instrument(skip(self), err)]
    pub fn command(&mut self, command: Command) -> Result<Response, MpdProtocolError>
    where
        IO: Read + Write,
    {
        self.send(command)?;

        if let Some(r) = self.receive()? {
            Ok(r)
        } else {
            error!("connection was closed without a response to the command");
            Err(MpdProtocolError::Io(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "connection was closed without a response to the command",
            )))
        }
    }

    /// Send a command list and receive its response(s).
    ///
    /// This is essentially a shorthand for [`Connection::send_list`] followed by [`Connection::receive`].
    ///
    /// # Errors
    ///
    /// This will return an error if:
    ///
    ///  - Writing to or reading from the connection returns an error
    ///  - Malformed response data is received
    ///  - The connection is closed
    #[tracing::instrument(skip(self), err)]
    pub fn command_list(&mut self, command_list: CommandList) -> Result<Response, MpdProtocolError>
    where
        IO: Read + Write,
    {
        self.send_list(command_list)?;

        if let Some(r) = self.receive()? {
            Ok(r)
        } else {
            error!("connection was closed without a response to the command");
            Err(MpdProtocolError::Io(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "connection was closed without a response to the command",
            )))
        }
    }

    /// Returns the protocol version the server is using.
    pub fn protocol_version(&self) -> &str {
        &self.protocol_version
    }

    /// Extract the connection instance.
    pub fn into_inner(self) -> IO {
        self.io
    }
}

fn read_to_buffer<'a, R: Read>(
    mut io: R,
    buf: &'a mut BytesMut,
    total: &mut usize,
) -> Result<(&'a [u8], usize), io::Error> {
    let read = io.read(&mut buf[*total..])?;
    trace!(read);
    *total += read;

    if buf.len() == *total {
        trace!("need to grow buffer");
        buf.resize(buf.len() * 2, 0);
    }

    Ok((&buf[..*total], read))
}

/// An **asynchronous** connection to an MPD server.
#[cfg(feature = "async")]
#[cfg_attr(docsrs, doc(cfg(feature = "async")))]
#[derive(Debug)]
pub struct AsyncConnection<IO>(Connection<IO>);

#[cfg(feature = "async")]
impl<IO> AsyncConnection<IO> {
    /// Connect to an MPD server asynchronously.
    ///
    /// # Errors
    ///
    /// This will return an error if:
    ///
    ///  - Reading from the given IO resource returns an error
    ///  - A malformed greeting is received
    ///  - The connection is closed before a complete greeting could be read
    #[cfg_attr(docsrs, doc(cfg(feature = "async")))]
    #[tracing::instrument(skip_all, err)]
    pub async fn connect(mut io: IO) -> Result<AsyncConnection<IO>, MpdProtocolError>
    where
        IO: AsyncRead + Unpin,
    {
        let mut recv_buf = BytesMut::with_capacity(DEFAULT_BUFFER_CAPACITY);

        let protocol_version = loop {
            let read = io.read_buf(&mut recv_buf).await?;
            trace!(read);

            if read == 0 {
                return Err(MpdProtocolError::Io(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "unexpected end of file while receiving greeting",
                )));
            }

            match parser::greeting(&recv_buf) {
                Ok((_, version)) => {
                    info!(?version, "connected successfully");
                    break Box::from(version);
                }
                Err(e) if e.is_incomplete() => {
                    // The response was valid *so far*, try another read
                    trace!("greeting incomplete");
                }
                Err(_) => {
                    error!("invalid greeting");
                    return Err(MpdProtocolError::InvalidMessage);
                }
            }
        };

        recv_buf.clear();

        Ok(AsyncConnection(Connection {
            io,
            protocol_version,
            field_cache: ResponseFieldCache::new(),
            recv_buf,
            total_received: 0,
        }))
    }

    /// Send a command.
    ///
    /// # Errors
    ///
    /// This will return an error if writing to the given IO resource fails.
    #[cfg_attr(docsrs, doc(cfg(feature = "async")))]
    #[tracing::instrument(skip(self), err)]
    pub async fn send(&mut self, mut command: Command) -> Result<(), MpdProtocolError>
    where
        IO: AsyncWrite + Unpin,
    {
        command.0.put_u8(b'\n');
        self.0.io.write_all(&command.0).await?;
        debug!(length = command.0.len(), "sent command");
        Ok(())
    }

    /// Send a command list.
    ///
    /// # Errors
    ///
    /// This will return an error if writing to the given IO resource fails.
    #[cfg_attr(docsrs, doc(cfg(feature = "async")))]
    #[tracing::instrument(skip(self), err)]
    pub async fn send_list(&mut self, command_list: CommandList) -> Result<(), MpdProtocolError>
    where
        IO: AsyncWrite + Unpin,
    {
        let buf = command_list.render();
        self.0.io.write_all(&buf).await?;
        debug!(length = buf.len(), "sent command");
        Ok(())
    }

    /// Receive a response from the server.
    ///
    /// This will return `Ok(Some(..))` when a complete response has been received, or `Ok(None)` if
    /// the connection is closed cleanly.
    ///
    /// # Errors
    ///
    /// This will return an error if:
    ///
    ///  - Reading from the given IO resource returns an error
    ///  - Malformed response data is received
    ///  - The connection is closed while a response is in progress
    #[cfg_attr(docsrs, doc(cfg(feature = "async")))]
    #[tracing::instrument(skip(self), err)]
    pub async fn receive(&mut self) -> Result<Option<Response>, MpdProtocolError>
    where
        IO: AsyncRead + Unpin,
    {
        let mut response_builder = ResponseBuilder::new(&mut self.0.field_cache);

        loop {
            if let Some(response) = response_builder.parse(&mut self.0.recv_buf)? {
                debug!(
                    frames = response.successful_frames(),
                    fields = response.field_count(),
                    error = response.is_error(),
                    "received complete response"
                );
                break Ok(Some(response));
            }

            let read = self.0.io.read_buf(&mut self.0.recv_buf).await?;
            trace!(read);

            if read == 0 {
                break if response_builder.is_frame_in_progress() || !self.0.recv_buf.is_empty() {
                    error!("EOF while receiving response");
                    Err(MpdProtocolError::Io(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "unexpected end of file while receiving response",
                    )))
                } else {
                    debug!("clean EOF while receiving");
                    Ok(None)
                };
            }
        }
    }

    /// Send a command and receive its response.
    ///
    /// This is essentially a shorthand for [`AsyncConnection::send`] followed by
    /// [`AsyncConnection::receive`].
    ///
    /// # Errors
    ///
    /// This will return an error if:
    ///
    ///  - Writing to or reading from the connection returns an error
    ///  - Malformed response data is received
    ///  - The connection is closed
    #[tracing::instrument(skip(self), err)]
    pub async fn command(&mut self, command: Command) -> Result<Response, MpdProtocolError>
    where
        IO: AsyncRead + AsyncWrite + Unpin,
    {
        self.send(command).await?;

        if let Some(r) = self.receive().await? {
            Ok(r)
        } else {
            error!("connection was closed without a response to the command");
            Err(MpdProtocolError::Io(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "connection was closed without a response to the command",
            )))
        }
    }

    /// Send a command list and receive its response(s).
    ///
    /// This is essentially a shorthand for [`AsyncConnection::send_list`] followed by
    /// [`AsyncConnection::receive`].
    ///
    /// # Errors
    ///
    /// This will return an error if:
    ///
    ///  - Writing to or reading from the connection returns an error
    ///  - Malformed response data is received
    ///  - The connection is closed
    #[tracing::instrument(skip(self), err)]
    pub async fn command_list(
        &mut self,
        command_list: CommandList,
    ) -> Result<Response, MpdProtocolError>
    where
        IO: AsyncRead + AsyncWrite + Unpin,
    {
        self.send_list(command_list).await?;

        if let Some(r) = self.receive().await? {
            Ok(r)
        } else {
            error!("connection was closed without a response to the command");
            Err(MpdProtocolError::Io(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "connection was closed without a response to the command",
            )))
        }
    }

    /// Returns the protocol version the server is using.
    pub fn protocol_version(&self) -> &str {
        &self.0.protocol_version
    }

    /// Extract the connection instance.
    pub fn into_inner(self) -> IO {
        self.0.io
    }
}

#[cfg(test)]
mod tests_sync {
    use assert_matches::assert_matches;

    use super::*;

    fn new_conn<IO>(io: IO) -> Connection<IO> {
        Connection {
            io,
            field_cache: ResponseFieldCache::new(),
            protocol_version: Box::from(""),
            recv_buf: BytesMut::zeroed(DEFAULT_BUFFER_CAPACITY),
            total_received: 0,
        }
    }

    #[test]
    fn connect() {
        let io: &[u8] = b"OK MPD 0.23.3\n";
        let connection = Connection::connect(io).unwrap();
        assert_eq!(connection.protocol_version(), "0.23.3");
    }

    #[test]
    fn connect_eof() {
        let io: &[u8] = b"OK MPD 0.23.3";
        let connection = Connection::connect(io).unwrap_err();
        assert_matches!(connection, MpdProtocolError::Io(e) if e.kind() == io::ErrorKind::UnexpectedEof);
    }

    #[test]
    fn connect_invalid() {
        let io: &[u8] = b"foobar\n";
        let connection = Connection::connect(io).unwrap_err();
        assert_matches!(connection, MpdProtocolError::InvalidMessage);
    }

    #[test]
    fn send() {
        let mut io = Vec::new();
        let mut connection = new_conn(&mut io);

        connection
            .send(Command::new("foo").argument("bar"))
            .unwrap();

        assert_eq!(io, b"foo bar\n");
    }

    #[test]
    fn send_list() {
        let mut io = Vec::new();
        let mut connection = new_conn(&mut io);

        let list = CommandList::new(Command::new("foo")).command(Command::new("bar"));

        connection.send_list(list).unwrap();

        assert_eq!(
            io,
            b"command_list_ok_begin\n\
              foo\n\
              bar\n\
              command_list_end\n"
        );
    }

    #[test]
    fn receive() {
        let io: &[u8] = b"foo: bar\nOK\n";
        let mut connection = new_conn(io);

        let response = connection.receive();

        assert_matches!(response, Ok(Some(_)));
    }

    #[test]
    fn receive_eof() {
        let io: &[u8] = b"foo: bar\nOK";
        let mut connection = new_conn(io);

        let response = connection.receive();

        assert_matches!(response, Err(MpdProtocolError::Io(e)) if e.kind() == io::ErrorKind::UnexpectedEof);
    }
}

#[cfg(test)]
#[cfg(feature = "async")]
mod tests_async {
    use assert_matches::assert_matches;
    use tokio_test::io::Builder as MockBuilder;

    use super::*;

    fn new_conn<IO>(io: IO) -> AsyncConnection<IO> {
        AsyncConnection(Connection {
            io,
            field_cache: ResponseFieldCache::new(),
            protocol_version: Box::from(""),
            recv_buf: BytesMut::new(),
            total_received: 0,
        })
    }

    #[tokio::test]
    async fn connect() {
        let io = MockBuilder::new().read(b"OK MPD 0.23.3\n").build();
        let connection = AsyncConnection::connect(io).await.unwrap();
        assert_eq!(connection.protocol_version(), "0.23.3");
    }

    #[tokio::test]
    async fn connect_split_read() {
        let io = MockBuilder::new()
            .read(b"OK MPD 0.23.3")
            .read(b"\n")
            .build();
        let connection = AsyncConnection::connect(io).await.unwrap();
        assert_eq!(connection.protocol_version(), "0.23.3");
    }

    #[tokio::test]
    async fn connect_eof() {
        let io = MockBuilder::new().read(b"OK MPD 0.23.3").build(); // no newline
        let connection = AsyncConnection::connect(io).await.unwrap_err();
        assert_matches!(connection, MpdProtocolError::Io(e) if e.kind() == io::ErrorKind::UnexpectedEof);
    }

    #[tokio::test]
    async fn connect_invalid() {
        let io = MockBuilder::new().read(b"OK foobar\n").build();
        let connection = AsyncConnection::connect(io).await.unwrap_err();
        assert_matches!(connection, MpdProtocolError::InvalidMessage);
    }

    #[tokio::test]
    async fn send_single() {
        let io = MockBuilder::new().write(b"status\n").build();
        let mut connection = new_conn(io);

        connection.send(Command::new("status")).await.unwrap();
    }

    #[tokio::test]
    async fn send_list() {
        let list = CommandList::new(Command::new("foo")).command(Command::new("bar"));
        let io = MockBuilder::new()
            .write(
                b"command_list_ok_begin\n\
                  foo\n\
                  bar\n\
                  command_list_end\n",
            )
            .build();
        let mut connection = new_conn(io);

        connection.send_list(list).await.unwrap();
    }

    #[tokio::test]
    async fn send_list_single() {
        let list = CommandList::new(Command::new("foo"));
        let io = MockBuilder::new().write(b"foo\n").build(); // skips command list wrapping
        let mut connection = new_conn(io);

        connection.send_list(list).await.unwrap();
    }

    #[tokio::test]
    async fn receive() {
        let io = MockBuilder::new().read(b"foo: bar\nOK\n").build();
        let mut connection = new_conn(io);

        let response = connection.receive().await.unwrap();

        assert_matches!(response, Some(response) if response.is_success());
    }

    #[tokio::test]
    async fn receive_split_read() {
        let io = MockBuilder::new().read(b"foo: bar\nOK").read(b"\n").build();
        let mut connection = new_conn(io);

        let response = connection.receive().await.unwrap();

        assert_matches!(response, Some(response) if response.is_success());
    }

    #[tokio::test]
    async fn receive_eof_clean() {
        let io = MockBuilder::new().build();
        let mut connection = new_conn(io);

        let response = connection.receive().await.unwrap();

        assert_eq!(response, None);
    }

    #[tokio::test]
    async fn receive_eof() {
        let io = MockBuilder::new().read(b"foo: bar\n").build();
        let mut connection = new_conn(io);

        let error = connection.receive().await.unwrap_err();

        assert_matches!(error, MpdProtocolError::Io(e) if e.kind() == io::ErrorKind::UnexpectedEof);
    }

    #[tokio::test]
    async fn receive_multiple() {
        let io = MockBuilder::new().read(b"OK\nOK\n").build();
        let mut connection = new_conn(io);

        let response = connection.receive().await.unwrap();
        assert_matches!(response, Some(response) if response.is_success());

        let response = connection.receive().await.unwrap();
        assert_matches!(response, Some(response) if response.is_success());

        let response = connection.receive().await.unwrap();
        assert_matches!(response, None);
    }

    #[tokio::test]
    async fn command() {
        let io = MockBuilder::new()
            .write(b"foo\n")
            .read(b"bar: baz\nOK\n")
            .build();
        let mut connection = new_conn(io);

        let resp = connection.command(Command::new("foo")).await.unwrap();
        assert_eq!(resp.field_count(), 1);
    }
}
