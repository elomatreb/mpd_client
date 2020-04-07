//! The client implementation.

use futures::{
    future::{select, Either},
    sink::SinkExt,
    stream::{Stream, StreamExt},
};
use mpd_protocol::{response::Frame, Command, CommandList, MpdCodec, MpdCodecError, Response};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::{
        mpsc::{self, UnboundedReceiver, UnboundedSender},
        oneshot,
    },
};
use tokio_util::codec::{Decoder, Framed};

use std::fmt::Debug;

use crate::errors::{CommandError, StateChangeError};
use crate::util::Subsystem;

static IDLE: &str = "idle";
static CANCEL_IDLE: &str = "noidle";

type CommandResponder = oneshot::Sender<Result<Response, CommandError>>;

/// Client for MPD.
#[derive(Clone, Debug)]
pub struct Client(UnboundedSender<(CommandList, CommandResponder)>);

impl Client {
    /// Connect to the MPD server using the given connection.
    ///
    /// Returns a `Client` you can use to issue commands, and a stream of state change events as
    /// returned by MPD.
    ///
    /// Since this is generic over the connection type, you can use it with both TCP and Unix
    /// socket connections.
    ///
    /// # Panics
    ///
    /// Since this spawns a task internally, this will panic when called outside a tokio runtime.
    ///
    /// # Errors
    ///
    /// This will return an error if sending the initial commands over the given transport fails.
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
        C: AsyncRead + AsyncWrite + Send + 'static,
    {
        let (state_changes_sender, state_changes) = mpsc::unbounded_channel();
        let (commands, commands_receiver) = mpsc::unbounded_channel();

        let connection = connect(connection).await?;

        tokio::spawn(run_loop(
            connection,
            commands_receiver,
            state_changes_sender,
        ));

        Ok((Self(commands), state_changes))
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
}

async fn connect<C: AsyncRead + AsyncWrite + Unpin>(
    connection: C,
) -> Result<Framed<C, MpdCodec>, MpdCodecError> {
    let mut framed = MpdCodec::new().framed(connection);

    // Immediately send an idle command, to prevent the connection from timing out
    write.send(CommandList::new(Command::new(IDLE))).await?;

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
    let mut state = State::Idling;

    loop {
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
                                    let _ = state_changes.send(state_change);
                                }

                                if let Err(e) = connection.send(Command::new(IDLE)).await {
                                    let _ = state_changes.send(Err(e.into()));
                                    break;
                                }
                            }
                            Some(Err(e)) => {
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

                        // Cancel currently ongoing idle
                        if let Err(e) = connection.send(Command::new(CANCEL_IDLE)).await {
                            let _ = responder.send(Err(e.into()));
                            break;
                        }

                        // Response to CANCEL_IDLE above
                        match connection.next().await {
                            None => break,
                            Some(Ok(res)) => {
                                if let Some(state_change) = response_to_subsystem(res).transpose() {
                                    let _ = state_changes.send(state_change);
                                }
                            }
                            Some(Err(e)) => {
                                let _ = responder.send(Err(e.into()));
                                break;
                            }
                        }

                        // Actually send the command. This sets the state for the next loop
                        // iteration.
                        match connection.send(command).await {
                            Ok(_) => state = State::WaitingForCommandReply(responder),
                            Err(e) => {
                                let _ = responder.send(Err(e.into()));
                                break;
                            }
                        }
                    }
                }
            }
            State::WaitingForCommandReply(responder) => {
                // We're waiting for the response to the command associated with `responder`.

                let response = match connection.next().await {
                    None => break,
                    Some(res) => res,
                };

                let _ = responder.send(response.map_err(Into::into));

                // Start idling again
                state = State::Idling;
                if let Err(e) = connection.send(CommandList::new(Command::new(IDLE))).await {
                    let _ = state_changes.send(Err(e.into()));
                    break;
                }
            }
        }
    }
}

fn response_to_subsystem(res: Response) -> Result<Option<Subsystem>, StateChangeError> {
    let mut frame = res.single_frame()?;

    if frame.values.is_empty() {
        Ok(None)
    } else {
        let raw = frame
            .get("changed")
            .ok_or(StateChangeError::MissingChangedKey)?;

        Ok(Some(Subsystem::from(raw)))
    }
}
