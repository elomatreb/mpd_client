//! The client implementation.

use futures::sink::SinkExt;
use mpd_protocol::{response::Error as ErrorResponse, Command, MpdCodec, MpdCodecError, Response};
use tokio::{
    io::{split, AsyncRead, AsyncWrite},
    stream::{Stream, StreamExt},
    sync::{
        mpsc::{self, error::SendError as MpscSendError, UnboundedReceiver, UnboundedSender},
        oneshot::{self, error::RecvError as OneshotRecvError},
        Mutex,
    },
};
use tokio_util::codec::{FramedRead, FramedWrite};

use std::collections::VecDeque;
use std::error::Error;
use std::fmt;
use std::fmt::Debug;

use crate::util::Subsystem;

static IDLE: &str = "idle";
static CANCEL_IDLE: &str = "noidle";

type CommandResponder = oneshot::Sender<Result<Response, CommandError>>;

/// Client for MPD.
#[derive(Debug)]
pub struct Client(Mutex<UnboundedSender<(Command, CommandResponder)>>);

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
    pub fn connect<C>(
        connection: C,
    ) -> (
        Self,
        impl Stream<Item = Result<Subsystem, StateChangeError>>,
    )
    where
        C: AsyncRead + AsyncWrite + Send + 'static,
    {
        let (state_changes_sender, state_changes) = mpsc::unbounded_channel();
        let (commands, commands_receiver) = mpsc::unbounded_channel();

        tokio::spawn(run_loop(
            connection,
            commands_receiver,
            state_changes_sender,
        ));

        (Self(Mutex::new(commands)), state_changes)
    }

    /// Send the given command, and return the response to it.
    pub async fn command(&self, command: Command) -> Result<Response, CommandError> {
        let (tx, rx) = oneshot::channel();
        self.0.lock().await.send((command, tx))?;

        rx.await?
    }
}

/// Errors which can occur when issuing a command.
#[derive(Debug)]
pub enum CommandError {
    /// The connection to MPD is closed
    ConnectionClosed,
    /// Received or attempted to send an invalid message
    InvalidMessage(MpdCodecError),
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandError::ConnectionClosed => write!(f, "The connection is closed"),
            CommandError::InvalidMessage(_) => write!(f, "Invalid message"),
        }
    }
}

impl Error for CommandError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            CommandError::InvalidMessage(e) => Some(e),
            _ => None,
        }
    }
}

#[doc(hidden)]
impl From<MpdCodecError> for CommandError {
    fn from(e: MpdCodecError) -> Self {
        CommandError::InvalidMessage(e)
    }
}

#[doc(hidden)]
impl<T> From<MpscSendError<T>> for CommandError {
    fn from(_: MpscSendError<T>) -> Self {
        CommandError::ConnectionClosed
    }
}

#[doc(hidden)]
impl From<OneshotRecvError> for CommandError {
    fn from(_: OneshotRecvError) -> Self {
        CommandError::ConnectionClosed
    }
}

/// Errors which may occur while listening for state change events.
#[derive(Debug)]
pub enum StateChangeError {
    /// The connection to MPD is closed
    ConnectionClosed,
    /// The message was invalid
    InvalidMessage(MpdCodecError),
    /// The state change message contained an error frame
    ErrorMessage(ErrorResponse),
    /// The state message wasn't empty, but did not contain the expected `changed` key
    MissingChangedKey,
}

impl fmt::Display for StateChangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StateChangeError::ConnectionClosed => write!(f, "The connection was closed"),
            StateChangeError::InvalidMessage(_) => write!(f, "Invalid message"),
            StateChangeError::ErrorMessage(ErrorResponse { code, message, .. }) => write!(
                f,
                "Message contained an error frame (code {} - {:?})",
                code, message
            ),
            StateChangeError::MissingChangedKey => write!(f, "Message was missing 'changed' key"),
        }
    }
}

impl Error for StateChangeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            StateChangeError::InvalidMessage(e) => Some(e),
            _ => None,
        }
    }
}

#[doc(hidden)]
impl From<ErrorResponse> for StateChangeError {
    fn from(r: ErrorResponse) -> Self {
        StateChangeError::ErrorMessage(r)
    }
}

#[doc(hidden)]
impl From<MpdCodecError> for StateChangeError {
    fn from(e: MpdCodecError) -> Self {
        StateChangeError::InvalidMessage(e)
    }
}

async fn run_loop<C>(
    connection: C,
    mut commands: UnboundedReceiver<(Command, CommandResponder)>,
    state_changes: UnboundedSender<Result<Subsystem, StateChangeError>>,
) where
    C: AsyncRead + AsyncWrite,
{
    let (read, write) = split(connection);
    let mut read = FramedRead::new(read, MpdCodec::new());
    let mut write = FramedWrite::new(write, MpdCodec::new());

    let mut current_command_responder: Option<CommandResponder> = None;
    let mut command_queue = VecDeque::new();

    // Immediately send an idle command, to prevent timeouts
    if let Err(e) = write.send(Command::build(IDLE).unwrap()).await {
        let _ = state_changes.send(Err(e.into()));
        return; // If the initial command fails, we can't do anything
    }

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
                    if let Err(e) = write.send(Command::build(CANCEL_IDLE).unwrap()).await {
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
                    let msg = msg.map_err(Into::into).and_then(|res| response_to_subsystem(res));

                    // The notification may be empty if no changes occured.
                    if let Some(msg) = msg.transpose() {
                        // Just like above, the state change notification channel may be dropped.
                        let _ = state_changes.send(msg);
                    }
                }

                // Get the next command to send
                if let Some((command, responder)) = command_queue.pop_front() {
                    // If there is a command in the queue, send it immediately and store the
                    // responder
                    current_command_responder = Some(responder);

                    if let Err(e) = write.send(command).await {
                        let responder = current_command_responder.take().unwrap();
                        let _ = responder.send(Err(e.into()));
                    }
                } else {
                    // If there is no command in the queue, start idling
                    if let Err(e) = write.send(Command::build(IDLE).unwrap()).await {
                        let _ = state_changes.send(Err(e.into()));
                    }
                }
            }
        }
    }
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
