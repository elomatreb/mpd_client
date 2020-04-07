//! The client implementation.

use futures::{
    future::{select, Either},
    sink::SinkExt,
    stream::{Stream, StreamExt},
};
use mpd_protocol::{response::Frame, Command, CommandList, MpdCodec, MpdCodecError, Response};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::{TcpStream, ToSocketAddrs, UnixStream},
    sync::{
        mpsc::{self, UnboundedReceiver, UnboundedSender},
        oneshot,
    },
};
use tokio_util::codec::{Decoder, Framed};
use tracing::{debug, error, span, trace, warn, Level, Span};
use tracing_futures::Instrument;

use std::fmt::Debug;
use std::path::Path;
use std::sync::Arc;

use crate::errors::{CommandError, StateChangeError};
use crate::util::Subsystem;

static IDLE: &str = "idle";
static CANCEL_IDLE: &str = "noidle";

type CommandResponder = oneshot::Sender<Result<Response, CommandError>>;

/// Client for MPD.
#[derive(Clone, Debug)]
pub struct Client {
    commands_sender: UnboundedSender<(CommandList, CommandResponder)>,
    span: Arc<Span>,
}

impl Client {
    /// Connect to an MPD server at the given TCP address.
    ///
    /// See [`connect`] for details of the result.
    ///
    /// # Panics
    ///
    /// This panics for the same reasons as [`connect`].
    ///
    /// # Errors
    ///
    /// This returns errors in the same conditions as [`connect`], and if connecting to the given
    /// TCP address fails for any reason.
    ///
    /// [`connect`]: #method.connect
    pub async fn connect_to<A: ToSocketAddrs + Debug>(
        address: A,
    ) -> Result<
        (
            Self,
            impl Stream<Item = Result<Subsystem, StateChangeError>>,
        ),
        MpdCodecError,
    > {
        let span = span!(Level::DEBUG, "client connection", tcp_addr = ?address);
        let connection = TcpStream::connect(address).await?;

        Self::do_connect(connection, span).await
    }

    /// Connect to an MPD server using the Unix socket at the given path.
    ///
    /// See [`connect`] for details of the result.
    ///
    /// # Panics
    ///
    /// This panics for the same reasons as [`connect`].
    ///
    /// # Errors
    ///
    /// This returns errors in the same conditions as [`connect`], and if connecting to the Unix
    /// socket at the given address fails for any reason.
    ///
    /// [`connect`]: #method.connect
    pub async fn connect_unix<P: AsRef<Path>>(
        path: P,
    ) -> Result<
        (
            Self,
            impl Stream<Item = Result<Subsystem, StateChangeError>>,
        ),
        MpdCodecError,
    > {
        let span = span!(Level::DEBUG, "client connection", unix_addr = ?path.as_ref());
        let connection = UnixStream::connect(path).await?;

        Self::do_connect(connection, span).await
    }

    /// Connect to the MPD server using the given connection.
    ///
    /// Returns a `Client` you can use to issue commands, and a stream of state change events as
    /// returned by MPD.
    ///
    /// Since this is generic over the connection type, you can use it with both TCP and Unix
    /// socket connections.
    ///
    /// See also [`connect_to`] and [`connect_unix`] for the common connection case.
    ///
    /// # Panics
    ///
    /// Since this spawns a task internally, this will panic when called outside a tokio runtime.
    ///
    /// # Errors
    ///
    /// This will return an error if sending the initial commands over the given transport fails.
    ///
    /// [`connect_to`]: #method.connect_to
    /// [`connect_unix`]: #method.connect_unix
    pub async fn connect<C>(
        connection: C,
    ) -> Result<
        (
            Self,
            impl Stream<Item = Result<Subsystem, StateChangeError>>,
        ),
        MpdCodecError,
    >
    where
        C: AsyncRead + AsyncWrite + Send + Unpin + 'static,
    {
        Self::do_connect(connection, span!(Level::DEBUG, "client connection")).await
    }

    /// Send the given command, and return the response to it.
    ///
    /// # Errors
    ///
    /// This will return an error if the connection to MPD is closed (cleanly) or a protocol error
    /// occurs (including IO errors), or if the command results in an MPD error.
    pub async fn command(&self, command: Command) -> Result<Frame, CommandError> {
        self.do_send(CommandList::new(command))
            .await?
            .single_frame()
            .map_err(Into::into)
    }

    /// Send the given command list, and return the responses to the contained commands.
    ///
    /// # Errors
    ///
    /// Errors will be returned in the same conditions as with [`command`], but if *any* of the
    /// commands in the list return an error condition, the entire list will be treated as an
    /// error. You may recover possible succesful fields in a response from the [error variant].
    ///
    /// [`command`]: #method.command
    /// [error variant]: ../errors/enum.CommandError.html#variant.ErrorResponse
    pub async fn command_list(&self, commands: CommandList) -> Result<Vec<Frame>, CommandError> {
        let res = self.do_send(commands).await?;
        let mut frames = Vec::with_capacity(res.len());

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

    async fn do_send(&self, commands: CommandList) -> Result<Response, CommandError> {
        let (tx, rx) = oneshot::channel();
        self.commands_sender.send((commands, tx))?;

        rx.await?
    }

    async fn do_connect<C>(
        connection: C,
        span: Span,
    ) -> Result<
        (
            Self,
            impl Stream<Item = Result<Subsystem, StateChangeError>>,
        ),
        MpdCodecError,
    >
    where
        C: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let (state_changes_sender, state_changes) = mpsc::unbounded_channel();
        let (commands_sender, commands_receiver) = mpsc::unbounded_channel();

        let connection = connect(connection).await?;

        debug!("connected succesfully");

        let run_loop = run_loop(connection, commands_receiver, state_changes_sender)
            .instrument(span!(parent: &span, Level::TRACE, "run loop"));

        tokio::spawn(run_loop);

        let client = Self {
            commands_sender,
            span: Arc::new(span),
        };

        Ok((client, state_changes))
    }
}

async fn connect<C: AsyncRead + AsyncWrite + Unpin>(
    connection: C,
) -> Result<Framed<C, MpdCodec>, MpdCodecError> {
    trace!("sending initial command");
    let mut framed = MpdCodec::new().framed(connection);

    // Immediately send an idle command, to prevent the connection from timing out
    if let Err(error) = framed.send(Command::new(IDLE)).await {
        error!(?error, "failed to send initial command");
    }

    Ok(framed)
}

#[derive(Debug)]
enum State {
    Idling,
    WaitingForCommandReply(CommandResponder),
}

async fn run_loop<C>(
    mut connection: Framed<C, MpdCodec>,
    mut commands: UnboundedReceiver<(CommandList, CommandResponder)>,
    state_changes: UnboundedSender<Result<Subsystem, StateChangeError>>,
) where
    C: AsyncRead + AsyncWrite + Unpin,
{
    trace!("entering run loop");

    let mut state = State::Idling;

    loop {
        trace!(?state, "loop iteration");

        match state {
            State::Idling => {
                // We are idling (the last command sent to the server was an IDLE).

                // Wait for either a command to send or a message from the server, which would be a
                // state change notification.
                let event = select(connection.next(), commands.next()).await;

                match event {
                    Either::Left((response, _)) => {
                        // A server message was received. Since we were idling, this is a state
                        // change notification or `None` is the connection was closed.

                        match response {
                            Some(Ok(res)) => {
                                if let Some(state_change) = response_to_subsystem(res).transpose() {
                                    trace!(?state_change);
                                    let _ = state_changes.send(state_change);
                                }

                                if let Err(e) = connection.send(Command::new(IDLE)).await {
                                    error!(error = ?e, "failed to start idling after state change");
                                    let _ = state_changes.send(Err(e.into()));
                                    break;
                                }
                            }
                            Some(Err(e)) => {
                                error!(error = ?e, "state change error");
                                let _ = state_changes.send(Err(e.into()));
                                break;
                            }
                            None => break, // The connection was closed
                        }
                    }
                    Either::Right((command, _)) => {
                        // A command was received or the commands channel was dropped. The latter
                        // is an indicator for us to close the connection.

                        let (command, responder) = match command {
                            None => break, // The connection was closed
                            Some(c) => c,
                        };

                        trace!(?command, "command received");

                        // Cancel currently ongoing idle
                        if let Err(e) = connection.send(Command::new(CANCEL_IDLE)).await {
                            error!(error = ?e, "failed to cancel idle prior to sending command");
                            let _ = responder.send(Err(e.into()));
                            break;
                        }

                        // Response to CANCEL_IDLE above
                        match connection.next().await {
                            None => break,
                            Some(Ok(res)) => {
                                if let Some(state_change) = response_to_subsystem(res).transpose() {
                                    trace!(?state_change);
                                    let _ = state_changes.send(state_change);
                                }
                            }
                            Some(Err(e)) => {
                                error!(error = ?e, "state change error prior to sending command");
                                let _ = responder.send(Err(e.into()));
                                break;
                            }
                        }

                        // Actually send the command. This sets the state for the next loop
                        // iteration.
                        match connection.send(command).await {
                            Ok(_) => state = State::WaitingForCommandReply(responder),
                            Err(e) => {
                                error!(error = ?e, "failed to send command");
                                let _ = responder.send(Err(e.into()));
                                break;
                            }
                        }

                        trace!("command sent succesfully");
                    }
                }
            }
            State::WaitingForCommandReply(responder) => {
                // We're waiting for the response to the command associated with `responder`.

                let response = match connection.next().await {
                    None => break,
                    Some(res) => res,
                };

                trace!(?response, "response to command received");

                let _ = responder.send(response.map_err(Into::into));

                // Start idling again
                state = State::Idling;
                if let Err(e) = connection.send(CommandList::new(Command::new(IDLE))).await {
                    error!(error = ?e, "failed to start idling after receiving command response");
                    let _ = state_changes.send(Err(e.into()));
                    break;
                }
            }
        }
    }

    trace!("exited run_loop");
}

fn response_to_subsystem(res: Response) -> Result<Option<Subsystem>, StateChangeError> {
    let mut frame = res.single_frame()?;

    if frame.values.is_empty() {
        Ok(None)
    } else {
        let raw = frame.get("changed").ok_or_else(|| {
            warn!("state change response was not empty but was missing the `changed` key");
            StateChangeError::MissingChangedKey
        })?;

        Ok(Some(Subsystem::from(raw)))
    }
}
