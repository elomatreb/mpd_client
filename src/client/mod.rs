//! The client implementation.

mod connection;

use mpd_protocol::Response as RawResponse;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::{TcpStream, ToSocketAddrs},
    sync::{
        mpsc::{self, Sender},
        oneshot,
    },
};
use tracing::{debug, span, trace, warn, Level, Span};
use tracing_futures::Instrument;

#[cfg(unix)]
use tokio::net::UnixStream;

use std::fmt::Debug;
#[cfg(unix)]
use std::path::Path;
use std::sync::Arc;

use crate::commands::{self as cmds, responses::Response, Command, CommandList};
use crate::errors::CommandError;
use crate::raw::{Frame, MpdProtocolError, RawCommand, RawCommandList};
use crate::state_changes::StateChanges;

type CommandResponder = oneshot::Sender<Result<RawResponse, CommandError>>;

/// Result returned by a connection attempt.
///
/// If successful, this contains a [`Client`] which you can use to issue commands, and a
/// [`StateChanges`] value, which is a stream that receives notifications from the server.
pub type ConnectResult = Result<(Client, StateChanges), MpdProtocolError>;

/// A client connected to an MPD instance.
///
/// You can use this to send commands to the MPD server, and wait for the response. Cloning the
/// `Client` will reuse the connection, similar to how a channel sender works.
///
/// # Connection management
///
/// Dropping the last clone of a particular `Client` will close the connection automatically.
///
/// # Example
///
/// ```no_run
/// use mpd_client::{commands::Play, Client};
///
/// async fn connect_to_mpd() -> Result<(), Box<dyn std::error::Error>> {
///     let (client, _state_changes) = Client::connect_to("localhost:6600").await?;
///
///     client.command(Play::current()).await?;
///     Ok(())
/// }
/// ```
#[derive(Clone, Debug)]
pub struct Client {
    commands_sender: Sender<(RawCommandList, CommandResponder)>,
    protocol_version: Arc<str>,
    span: Span,
}

impl Client {
    /// Connect to an MPD server at the given TCP address.
    ///
    /// This requires the `dns` [Tokio feature][tokio-features] to resolve domain names.
    ///
    /// # Panics
    ///
    /// This panics for the same reasons as [`Client::connect`].
    ///
    /// # Errors
    ///
    /// This returns errors in the same conditions as [`Client::connect`], and if connecting to the given
    /// TCP address fails for any reason.
    ///
    /// [tokio-features]: https://docs.rs/tokio/0.2/tokio/#feature-flags
    pub async fn connect_to<A: ToSocketAddrs + Debug>(address: A) -> ConnectResult {
        let span = span!(Level::DEBUG, "client connection", tcp_addr = ?address);
        let connection = TcpStream::connect(address).await?;

        Self::do_connect(connection, span).await
    }

    /// Connect to an MPD server using the Unix socket at the given path.
    ///
    /// This is only available on Unix.
    ///
    /// # Panics
    ///
    /// This panics for the same reasons as [`Client::connect`].
    ///
    /// # Errors
    ///
    /// This returns errors in the same conditions as [`Client::connect`], and if connecting to the Unix
    /// socket at the given address fails for any reason.
    #[cfg(unix)]
    pub async fn connect_unix<P: AsRef<Path>>(path: P) -> ConnectResult {
        let span = span!(Level::DEBUG, "client connection", unix_addr = ?path.as_ref());
        let connection = UnixStream::connect(path).await?;

        Self::do_connect(connection, span).await
    }

    /// Connect to the MPD server using the given connection.
    ///
    /// Since this method is generic over the connection type it can be used to connect to either a
    /// TCP or Unix socket dynamically or e.g. use a proxy.
    ///
    /// See also the [`Client::connect_to`] and [`Client::connect_unix`] shorthands for the common
    /// connection case.
    ///
    /// # Panics
    ///
    /// Since this spawns a task internally, this will panic when called outside a Tokio runtime.
    ///
    /// # Errors
    ///
    /// This will return an error if sending the initial commands over the given transport fails.
    pub async fn connect<C>(connection: C) -> ConnectResult
    where
        C: AsyncRead + AsyncWrite + Send + Unpin + 'static,
    {
        Self::do_connect(connection, span!(Level::DEBUG, "client connection")).await
    }

    /// Send a [command].
    ///
    /// This will automatically parse the response to a proper type.
    ///
    /// # Errors
    ///
    /// This returns errors in the same conditions as [`Client::raw_command`], and additionally if the
    /// response fails to convert to the expected type.
    ///
    /// [command]: super::commands
    pub async fn command<C>(&self, cmd: C) -> Result<C::Response, CommandError>
    where
        C: Command,
    {
        let command = cmd.into_command();
        let frame = self.raw_command(command).await?;

        Ok(Response::from_frame(frame)?)
    }

    /// Send the given command list, and return the (typed) responses.
    ///
    /// # Errors
    ///
    /// This returns errors in the same conditions as [`Client::raw_command_list`], and
    /// additionally if the response type conversion fails.
    pub async fn command_list<L>(&self, list: L) -> Result<L::Response, CommandError>
    where
        L: CommandList,
    {
        let frames = match list.into_raw_command_list() {
            Some(cmds) => self.raw_command_list(cmds).await?,
            None => Vec::new(),
        };

        <L as CommandList>::parse_responses(frames).map_err(Into::into)
    }

    /// Send the connection password, if necessary.
    ///
    /// If the MPD instance the `Client` is connected to is protected by a password, sending
    /// commands may result in "Permission denied" errors prior to sending the correct password.
    ///
    /// This will not send anything if `None` is passed.
    ///
    /// # Errors
    ///
    /// Sending an incorrect password will result in a [`CommandError::ErrorResponse`], any other
    /// errors are returned in the same conditions as [`Client::raw_command`].
    pub async fn password(&self, password: Option<String>) -> Result<(), CommandError> {
        if let Some(pw) = password {
            self.command(cmds::Password(pw)).await
        } else {
            Ok(())
        }
    }

    /// Send the given command, and return the response to it.
    ///
    /// # Errors
    ///
    /// This will return an error if the connection to MPD is closed (cleanly) or a protocol error
    /// occurs (including IO errors), or if the command results in an MPD error.
    pub async fn raw_command(&self, command: RawCommand) -> Result<Frame, CommandError> {
        self.do_send(RawCommandList::new(command))
            .await?
            .single_frame()
            .map_err(Into::into)
    }

    /// Send the given command list, and return the raw response frames to the contained commands.
    ///
    /// # Errors
    ///
    /// Errors will be returned in the same conditions as with [`Client::raw_command`], but if
    /// *any* of the commands in the list return an error condition, the entire list will be
    /// treated as an error.
    ///
    /// You may recover possible succesful fields in a response from the [error].
    ///
    /// [error]: crate::errors::CommandError::ErrorResponse
    pub async fn raw_command_list(
        &self,
        commands: RawCommandList,
    ) -> Result<Vec<Frame>, CommandError> {
        debug!(?commands, "sending command");

        let res = self.do_send(commands).await?;
        let mut frames = Vec::with_capacity(res.successful_frames());

        for frame in res {
            match frame {
                Ok(f) => frames.push(f),
                Err(error) => {
                    return Err(CommandError::ErrorResponse {
                        error,
                        succesful_frames: frames,
                    });
                }
            }
        }

        Ok(frames)
    }

    /// Load album art for the given URI.
    ///
    /// # Behavior
    ///
    /// This first tries to use the [`readpicture`][cmds::AlbumArtEmbedded] command to load
    /// embedded data, before falling back to reading from a separate file using the
    /// [`albumart`](cmds::AlbumArt) command.
    ///
    /// **Note**: Due to the default binary size limit of MPD being quite low, loading larger art
    /// will issue many commands and can be slow. Consider increasing the
    /// [binary size limit][cmds::SetBinaryLimit].
    ///
    /// # Return value
    ///
    /// If this method returns succesfully, a return value of  `None` indicates that no album art
    /// for the given URI was found. Otherwise, you will get a tuple consisting of the raw binary
    /// data, and an optional string value that contains a MIME type for the data, if one was
    /// provided by the server.
    ///
    /// # Errors
    ///
    /// This returns errors in the same conditions as [`Client::command`].
    pub async fn album_art(
        &self,
        uri: &str,
    ) -> Result<Option<(Vec<u8>, Option<String>)>, CommandError> {
        let span = span!(Level::DEBUG, "album_art", ?uri);
        let _enter = span.enter();

        debug!("loading album art");

        let mut out = Vec::new();
        let mut expected_size = 0;
        let mut embedded = false;
        let mut mime = None;

        match self
            .command(cmds::AlbumArtEmbedded::new(uri.to_owned()))
            .await
        {
            Ok(Some(resp)) => {
                expected_size = resp.size;
                out.reserve(expected_size);
                out.extend_from_slice(resp.data());
                embedded = true;
                mime = resp.mime;
                debug!(length = resp.size, ?mime, "found embedded album art");
            }
            Ok(None) => {
                debug!("readpicture command gave no result, falling back");
            }
            Err(e) => match e {
                CommandError::ErrorResponse { error, .. } if error.code == 5 => {
                    debug!("readpicture command unsupported, falling back");
                }
                e => return Err(e),
            },
        }

        if !embedded {
            if let Some(resp) = self.command(cmds::AlbumArt::new(uri.to_owned())).await? {
                expected_size = resp.size;
                out.reserve(expected_size);
                out.extend_from_slice(resp.data());
                debug!(length = expected_size, "found separate file album art");
            } else {
                debug!("no embedded or separte album art found");
                return Ok(None);
            }
        }

        while out.len() < expected_size {
            let resp = if embedded {
                self.command(cmds::AlbumArtEmbedded::new(uri.to_owned()).offset(out.len()))
                    .await?
            } else {
                self.command(cmds::AlbumArt::new(uri.to_owned()).offset(out.len()))
                    .await?
            };

            if let Some(resp) = resp {
                let data = resp.data();
                trace!(received = data.len(), progress = out.len());
                out.extend_from_slice(data);
            } else {
                warn!(progress = out.len(), "incomplete cover art response");
                return Ok(None);
            }
        }

        debug!(length = expected_size, "finished loading");

        Ok(Some((out, mime)))
    }

    /// Get the protocol version the underlying connection is using.
    pub fn protocol_version(&self) -> &str {
        self.protocol_version.as_ref()
    }

    async fn do_send(&self, commands: RawCommandList) -> Result<RawResponse, CommandError> {
        let (tx, rx) = oneshot::channel();

        self.commands_sender.send((commands, tx)).await?;

        rx.await?
    }

    async fn do_connect<C>(
        connection: C,
        span: Span,
    ) -> Result<(Self, StateChanges), MpdProtocolError>
    where
        C: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let (state_changes_sender, state_changes) = mpsc::unbounded_channel();
        let (commands_sender, commands_receiver) = mpsc::channel(1);

        let connection = connection::connect(connection, &span).await?;

        let protocol_version = Arc::from(connection.codec().protocol_version());

        tokio::spawn(
            connection::run_loop(connection, commands_receiver, state_changes_sender)
                .instrument(span!(parent: &span, Level::TRACE, "run loop")),
        );

        let state_changes = StateChanges { rx: state_changes };
        let client = Client {
            commands_sender,
            protocol_version,
            span,
        };

        Ok((client, state_changes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state_changes::Subsystem;
    use futures::StreamExt;
    use tokio_test::{assert_ok, io::Builder as MockBuilder};

    static GREETING: &[u8] = b"OK MPD 0.21.11\n";

    #[tokio::test]
    async fn single_state_change() {
        let io = MockBuilder::new()
            .read(GREETING)
            .write(b"idle\n")
            .read(b"changed: player\nOK\n")
            .write(b"idle\n")
            .build();

        let (_client, mut state_changes) = Client::connect(io).await.expect("connect failed");

        assert_eq!(
            assert_ok!(state_changes.next().await.expect("no state change")),
            Subsystem::Player
        );
    }

    #[tokio::test]
    async fn command() {
        let io = MockBuilder::new()
            .read(GREETING)
            .write(b"idle\n")
            .write(b"noidle\n")
            .read(b"changed: playlist\nOK\n")
            .write(b"hello\n")
            .read(b"foo: bar\nOK\n")
            .write(b"idle\n")
            .build();

        let (client, mut state_changes) = Client::connect(io).await.expect("connect failed");

        let response = client
            .raw_command(RawCommand::new("hello"))
            .await
            .expect("command failed");

        assert_eq!(response.find("foo"), Some("bar"));
        assert_eq!(
            assert_ok!(state_changes.next().await.expect("no state change")),
            Subsystem::Queue
        );
        assert!(state_changes.next().await.is_none());
    }

    #[tokio::test]
    async fn incomplete_response() {
        let io = MockBuilder::new()
            .read(GREETING)
            .write(b"idle\n")
            .write(b"noidle\n")
            .read(b"OK\n")
            .write(b"hello\n")
            .read(b"foo: bar\n")
            .read(b"baz: qux\nOK\n")
            .write(b"idle\n")
            .build();

        let (client, _state_changes) = Client::connect(io).await.expect("connect failed");

        let response = client
            .raw_command(RawCommand::new("hello"))
            .await
            .expect("command failed");

        assert_eq!(response.find("foo"), Some("bar"));
    }

    #[tokio::test]
    async fn command_list() {
        let io = MockBuilder::new()
            .read(GREETING)
            .write(b"idle\n")
            .write(b"noidle\n")
            .read(b"OK\n")
            .write(b"command_list_ok_begin\nfoo\nbar\ncommand_list_end\n")
            .read(b"foo: asdf\nlist_OK\n")
            .read(b"baz: qux\nlist_OK\nOK\n")
            .write(b"idle\n")
            .build();

        let (client, _state_changes) = Client::connect(io).await.expect("connect failed");

        let mut commands = RawCommandList::new(RawCommand::new("foo"));
        commands.add(RawCommand::new("bar"));

        let responses = client
            .raw_command_list(commands)
            .await
            .expect("command failed");

        assert_eq!(responses.len(), 2);
        assert_eq!(responses[0].find("foo"), Some("asdf"));
    }

    #[tokio::test]
    async fn dropping_client() {
        let io = MockBuilder::new().read(GREETING).write(b"idle\n").build();

        let (client, mut state_changes) = Client::connect(io).await.expect("connect failed");

        drop(client);

        assert!(state_changes.next().await.is_none());
    }

    #[tokio::test]
    async fn album_art() {
        let io = MockBuilder::new()
            .read(GREETING)
            .write(b"idle\n")
            .write(b"noidle\n")
            .read(b"OK\n")
            .write(b"readpicture foo/bar.mp3 0\n")
            .read(b"size: 6\ntype: image/jpeg\nbinary: 3\nFOO\nOK\n")
            .write(b"readpicture foo/bar.mp3 3\n")
            .read(b"size: 6\ntype: image/jpeg\nbinary: 3\nBAR\nOK\n")
            .build();

        let (client, _) = Client::connect(io).await.expect("connect failed");

        let x = client
            .album_art("foo/bar.mp3")
            .await
            .expect("command failed");

        assert_eq!(
            x,
            Some((Vec::from("FOOBAR"), Some(String::from("image/jpeg"))))
        );
    }

    #[tokio::test]
    async fn album_art_fallback() {
        let io = MockBuilder::new()
            .read(GREETING)
            .write(b"idle\n")
            .write(b"noidle\n")
            .read(b"OK\n")
            .write(b"readpicture foo/bar.mp3 0\n")
            .read(b"OK\n")
            .write(b"albumart foo/bar.mp3 0\n")
            .read(b"size: 6\nbinary: 3\nFOO\nOK\n")
            .write(b"albumart foo/bar.mp3 3\n")
            .read(b"size: 6\nbinary: 3\nBAR\nOK\n")
            .build();

        let (client, _) = Client::connect(io).await.expect("connect failed");

        let x = client
            .album_art("foo/bar.mp3")
            .await
            .expect("command failed");

        assert_eq!(x, Some((Vec::from("FOOBAR"), None)));
    }

    #[tokio::test]
    async fn album_art_none() {
        let io = MockBuilder::new()
            .read(GREETING)
            .write(b"idle\n")
            .write(b"noidle\n")
            .read(b"OK\n")
            .write(b"readpicture foo/bar.mp3 0\n")
            .read(b"OK\n")
            .write(b"albumart foo/bar.mp3 0\n")
            .read(b"OK\n")
            .build();

        let (client, _) = Client::connect(io).await.expect("connect failed");

        let x = client
            .album_art("foo/bar.mp3")
            .await
            .expect("command failed");

        assert_eq!(x, None);
    }

    #[tokio::test]
    async fn protocol_version() {
        let io = MockBuilder::new().read(GREETING).write(b"idle\n").build();

        let (client, _state_changes) = Client::connect(io).await.expect("connect failed");

        assert_eq!(client.protocol_version(), "0.21.11");
    }
}
