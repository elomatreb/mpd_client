//! The client implementation.

use futures::sink::SinkExt;
use mpd_protocol::{response::Frame, Command, CommandList, MpdCodec, MpdCodecError, Response};
use tokio::{
    io::{self, split, AsyncRead, AsyncWrite},
    stream::{Stream, StreamExt},
    sync::{
        mpsc::{self, UnboundedReceiver, UnboundedSender},
        oneshot,
    },
};
use tokio_util::codec::{FramedRead, FramedWrite};

use std::collections::VecDeque;
use std::iter;

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
        self.0.send((commands, tx))?;

        rx.await?
    }
}

struct Connection<C: AsyncRead + AsyncWrite> {
    read: FramedRead<io::ReadHalf<C>, MpdCodec>,
    write: FramedWrite<io::WriteHalf<C>, MpdCodec>,
}

async fn connect<C: AsyncRead + AsyncWrite>(connection: C) -> Result<Connection<C>, MpdCodecError> {
    let (read, write) = split(connection);
    let read = FramedRead::new(read, MpdCodec::new());
    let mut write = FramedWrite::new(write, MpdCodec::new());

    // Immediately send an idle command, to prevent the connection from timing out
    write.send(CommandList::new(Command::new(IDLE))).await?;

    Ok(Connection { read, write })
}

async fn run_loop<C>(
    connection: Connection<C>,
    mut commands: UnboundedReceiver<(CommandList, CommandResponder)>,
    state_changes: UnboundedSender<Result<Subsystem, StateChangeError>>,
) where
    C: AsyncRead + AsyncWrite,
{
    let Connection {
        mut read,
        mut write,
    } = connection;

    let mut current_command_responder: Option<CommandResponder> = None;
    let mut command_queue = VecDeque::new();

    let mut commands_channel_open = true;

    while commands_channel_open || current_command_responder.is_some() || !command_queue.is_empty()
    {
        tokio::select! {
            command = commands.next(), if commands_channel_open => {
                let command = match command {
                    Some(c) => c,
                    None => {
                        commands_channel_open = false;
                        continue;
                    }
                };

                // Add the command to the queue to be processed during a later iteration of this
                // loop.
                command_queue.push_back(command);

                if current_command_responder.is_none() {
                    // If there is no current responder, we are currently idling, so cancel that.
                    if let Err(e) = write.send(CommandList::new(Command::new(CANCEL_IDLE))).await {
                        // Get back the responder (and command) we just pushed
                        let (_, responder) = command_queue.pop_back().unwrap();
                        let _ = responder.send(Err(e.into()));
                    }
                }
            }
            msg = read.next() => {
                let msg = match msg {
                    Some(m) => m,
                    // If the connection is closed, exit the loop. This will drop all remaining
                    // responders, which signals error conditions.
                    None => break,
                };

                // If there is a current responder set, the message we just received was a reply to
                // an explicit command.
                if let Some(responder) = current_command_responder.take() {
                    // The response channel may have already been dropped if the future
                    // representing the command request was cancelled dropped, but there's nothing
                    // we can do about it.
                    let _ = responder.send(msg.map_err(Into::into));
                } else {
                    // If not responder is available, the message is a state change notification.
                    let msg = msg.map_err(Into::into).and_then(response_to_subsystem);

                    // The notification may be empty if no changes occured.
                    if let Some(msg) = msg.transpose() {
                        // Just like above, the state change notification channel may be dropped.
                        let _ = state_changes.send(msg);
                    }
                }

                // Get the next command with an open response channel (closed channels represent
                // cancelled command futures).
                if let Some((command, responder)) = next_command(&mut command_queue) {
                    // If there is a command in the queue, send it immediately and store the
                    // responder
                    current_command_responder = Some(responder);

                    if let Err(e) = write.send(command).await {
                        let responder = current_command_responder.take().unwrap();
                        let _ = responder.send(Err(e.into()));
                    }
                } else {
                    // If there is no command in the queue, start idling
                    if let Err(e) = write.send(CommandList::new(Command::new(IDLE))).await {
                        let _ = state_changes.send(Err(e.into()));
                    }
                }
            }
        }
    }
}

fn next_command(
    queue: &mut VecDeque<(CommandList, CommandResponder)>,
) -> Option<(CommandList, CommandResponder)> {
    iter::from_fn(|| queue.pop_front())
        .find(|(_, responder)| !responder.is_closed())
}

fn response_to_subsystem(res: Response) -> Result<Option<Subsystem>, StateChangeError> {
    let values = res.single_frame()?.values;

    if values.is_empty() {
        Ok(None)
    } else {
        let raw = values
            .into_iter()
            .find(|(k, _)| k == "changed")
            .ok_or(StateChangeError::MissingChangedKey)?
            .1;

        Ok(Some(Subsystem::from(raw)))
    }
}
