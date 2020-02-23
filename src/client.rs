//! The client implementation.

use futures::sink::{Sink, SinkExt};
use mpd_protocol::{Command, MpdCodec, Response};
use tokio::{
    io::{split, AsyncRead, AsyncWrite},
    stream::{Stream, StreamExt},
    sync::{
        mpsc::{self, UnboundedReceiver, UnboundedSender},
        oneshot, Mutex,
    },
};
use tokio_util::codec::{FramedRead, FramedWrite};

use std::collections::VecDeque;
use std::fmt::Debug;

use crate::util::Subsystem;

static IDLE: &str = "idle";
static CANCEL_IDLE: &str = "noidle";

/// Client for MPD.
#[derive(Debug)]
pub struct Client(Mutex<UnboundedSender<(Command, oneshot::Sender<Response>)>>);

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
    pub fn connect<C>(connection: C) -> (Self, impl Stream<Item = Subsystem>)
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
    pub async fn command(&self, command: Command) -> Response {
        let (tx, rx) = oneshot::channel();
        self.0
            .lock()
            .await
            .send((command, tx))
            .expect("Sending command");
        rx.await.expect("Receiving command reply")
    }
}

async fn run_loop<C>(
    connection: C,
    mut commands: UnboundedReceiver<(Command, oneshot::Sender<Response>)>,
    state_changes: UnboundedSender<Subsystem>,
) where
    C: AsyncRead + AsyncWrite,
{
    let (read, write) = split(connection);
    let mut read = FramedRead::new(read, MpdCodec::new());
    let mut write = FramedWrite::new(write, MpdCodec::new());

    let mut current_command_responder: Option<oneshot::Sender<Response>> = None;
    let mut command_queue = VecDeque::new();

    // Immediately send an idle command, to prevent timeouts
    begin_idle(&mut write).await;

    loop {
        tokio::select! {
            command = commands.next() => {
                let command = match command {
                    Some(c) => c,
                    None => break, // The commands channel has been closed, exit cleanly.
                };

                // Add the command to the queue to be processed during a later iteration of this
                // loop.
                command_queue.push_back(command);

                if current_command_responder.is_none() {
                    // If there is no current responder, we are currently idling, so cancel that.
                    cancel_idle(&mut write).await;
                }
            }
            msg = read.next() => {
                let msg = msg.expect("MPD connection closed").expect("Invalid MPD response");

                // If there is a current responder set, the message we just received was a reply to
                // an explicit command.
                if let Some(responder) = current_command_responder.take() {
                    // The response channel may have already been dropped if the future
                    // representing the command request was cancelled dropped, but there's nothing
                    // we can do about it.
                    let _ = responder.send(msg);
                } else {
                    // If not responder is available, the message is a state change notification.

                    // The notification may be empty if no changes occured.
                    if let Some(subsystem) = response_to_subsystem(msg) {
                        // Just like above, the state change notification channel may be dropped.
                        let _ = state_changes.send(subsystem);
                    }
                }

                // Get the next command to send
                if let Some((command, responder)) = command_queue.pop_front() {
                    // If there is a command in the queue, send it immediately and store the
                    // responder
                    current_command_responder = Some(responder);
                    send_command(&mut write, command).await;
                } else {
                    // If there is no command in the queue, start idling
                    begin_idle(&mut write).await;
                }
            }
        }
    }
}

async fn cancel_idle<D>(dest: D)
where
    D: Sink<Command> + Unpin,
    <D as Sink<Command>>::Error: Debug,
{
    send_command(dest, Command::build(CANCEL_IDLE).unwrap()).await;
}

async fn begin_idle<D>(dest: D)
where
    D: Sink<Command> + Unpin,
    <D as Sink<Command>>::Error: Debug,
{
    send_command(dest, Command::build(IDLE).unwrap()).await;
}

async fn send_command<D>(mut dest: D, command: Command)
where
    D: Sink<Command> + Unpin,
    <D as Sink<Command>>::Error: Debug,
{
    dest.send(command).await.expect("sending command");
}

fn response_to_subsystem(res: Response) -> Option<Subsystem> {
    let mut values = res
        .single_frame()
        .expect("error in status change message")
        .values;

    match values.len() {
        0 => None, // The response is empty if no changes occured (when the idle is cancelled)
        1 => {
            let (key, value) = values.pop().unwrap();
            assert_eq!("changed", key, "status change message contained wrong key");
            Some(Subsystem::from(value))
        }
        len => panic!("status change message contained too many values: {}", len),
    }
}
