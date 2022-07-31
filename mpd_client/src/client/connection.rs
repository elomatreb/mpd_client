use std::{fmt, time::Duration};

use mpd_protocol::{
    command::{Command as RawCommand, CommandList as RawCommandList},
    response::Response,
    AsyncConnection, MpdProtocolError,
};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
    time::timeout,
};
use tracing::{debug, error, span, trace, Instrument, Level};

use crate::client::{CommandResponder, ConnectionError, ConnectionEvent, Subsystem};

struct State<C> {
    loop_state: LoopState,
    connection: AsyncConnection<C>,
    commands: UnboundedReceiver<(RawCommandList, CommandResponder)>,
    events: UnboundedSender<ConnectionEvent>,
}

enum LoopState {
    Idling,
    WaitingForCommandReply(CommandResponder),
}

impl fmt::Debug for LoopState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // avoid Debug-printing the noisy internals of the contained channel type
        match self {
            LoopState::Idling => write!(f, "Idling"),
            LoopState::WaitingForCommandReply(_) => write!(f, "WaitingForCommandReply"),
        }
    }
}

fn idle() -> RawCommand {
    RawCommand::new("idle")
}

fn cancel_idle() -> RawCommand {
    RawCommand::new("noidle")
}

pub(super) async fn run_loop<C>(
    mut connection: AsyncConnection<C>,
    commands: UnboundedReceiver<(RawCommandList, CommandResponder)>,
    events: UnboundedSender<ConnectionEvent>,
) where
    C: AsyncRead + AsyncWrite + Unpin,
{
    trace!("sending initial idle command");
    if let Err(e) = connection.send(idle()).await {
        error!(error = ?e, "failed to send initial idle command");
        let _ = events.send(ConnectionEvent::ConnectionClosed(e.into()));
        return;
    }

    let mut state = State {
        loop_state: LoopState::Idling,
        connection,
        commands,
        events,
    };

    trace!("entering run loop");

    loop {
        let span = span!(Level::TRACE, "iteration", state = ?state.loop_state);

        match run_loop_iteration(state).instrument(span).await {
            Ok(new_state) => state = new_state,
            Err(()) => break,
        }
    }

    trace!("exited run_loop");
}

/// Time to wait for another command to send before starting the idle loop.
const NEXT_COMMAND_IDLE_TIMEOUT: Duration = Duration::from_millis(100);

async fn run_loop_iteration<C>(mut state: State<C>) -> Result<State<C>, ()>
where
    C: AsyncRead + AsyncWrite + Unpin,
{
    match state.loop_state {
        LoopState::Idling => {
            // We are idling (the last command sent to the server was an IDLE).

            // Wait for either a command to send or a message from the server, which would be a
            // state change notification.
            tokio::select! {
                response = state.connection.receive() => {
                    handle_idle_response(&mut state, response).await?;
                }
                command = state.commands.recv() => {
                    handle_command(&mut state, command).await?;
                }
            }
        }
        LoopState::WaitingForCommandReply(responder) => {
            // We're waiting for the response to the command associated with `responder`.

            let response = state.connection.receive().await.transpose().ok_or(())?;
            trace!("response to command received");

            let _ = responder.send(response.map_err(Into::into));

            let next_command = timeout(NEXT_COMMAND_IDLE_TIMEOUT, state.commands.recv());

            // See if we can immediately send the next command
            match next_command.await {
                Ok(Some((command, responder))) => {
                    trace!(?command, "next command immediately available");
                    match state.connection.send_list(command).await {
                        Ok(_) => state.loop_state = LoopState::WaitingForCommandReply(responder),
                        Err(e) => {
                            error!(error = ?e, "failed to send command");
                            let _ = responder.send(Err(e.into()));
                            return Err(());
                        }
                    }
                }
                Ok(None) => return Err(()),
                Err(_) => {
                    trace!("reached next command timeout, idling");

                    // Start idling again
                    state.loop_state = LoopState::Idling;
                    if let Err(e) = state.connection.send(idle()).await {
                        error!(error = ?e, "failed to start idling after receiving command response");
                        let _ = state
                            .events
                            .send(ConnectionEvent::ConnectionClosed(e.into()));
                        return Err(());
                    }
                }
            }
        }
    }

    Ok(state)
}

async fn handle_command<C>(
    state: &mut State<C>,
    command: Option<(RawCommandList, CommandResponder)>,
) -> Result<(), ()>
where
    C: AsyncRead + AsyncWrite + Unpin,
{
    let (command, responder) = command.ok_or(())?;
    trace!(?command, "command received");

    // Cancel currently ongoing idle
    if let Err(e) = state.connection.send(cancel_idle()).await {
        error!(error = ?e, "failed to cancel idle prior to sending command");
        let _ = responder.send(Err(e.into()));
        return Err(());
    }

    // Receive the response to the cancellation
    match state.connection.receive().await {
        Ok(None) => return Err(()),
        Ok(Some(res)) => match res.into_single_frame() {
            Ok(f) => {
                if let Some(subsystem) = Subsystem::from_frame(f) {
                    debug!(?subsystem, "state change");
                    let _ = state
                        .events
                        .send(ConnectionEvent::SubsystemChange(subsystem));
                }
            }
            Err(e) => {
                error!(
                    code = e.code,
                    message = e.message,
                    "idle cancel returned an error"
                );
                let _ = state.events.send(ConnectionEvent::ConnectionClosed(
                    ConnectionError::InvalidResponse,
                ));
                return Err(());
            }
        },
        Err(e) => {
            error!(error = ?e, "state change error prior to sending command");
            let _ = responder.send(Err(e.into()));
            return Err(());
        }
    }

    // Actually send the command. This sets the state for the next loop
    // iteration.
    match state.connection.send_list(command).await {
        Ok(_) => state.loop_state = LoopState::WaitingForCommandReply(responder),
        Err(e) => {
            error!(error = ?e, "failed to send command");
            let _ = responder.send(Err(e.into()));
            return Err(());
        }
    }

    trace!("command sent successfully");
    Ok(())
}

async fn handle_idle_response<C>(
    state: &mut State<C>,
    response: Result<Option<Response>, MpdProtocolError>,
) -> Result<(), ()>
where
    C: AsyncRead + AsyncWrite + Unpin,
{
    trace!("handling idle response");

    match response {
        Ok(Some(res)) => {
            match res.into_single_frame() {
                Ok(f) => {
                    if let Some(subsystem) = Subsystem::from_frame(f) {
                        debug!(?subsystem, "state change");
                        let _ = state
                            .events
                            .send(ConnectionEvent::SubsystemChange(subsystem));
                    }
                }
                Err(e) => {
                    error!(code = e.code, message = e.message, "idle returned an error");
                    let _ = state.events.send(ConnectionEvent::ConnectionClosed(
                        ConnectionError::InvalidResponse,
                    ));
                    return Err(());
                }
            }

            if let Err(e) = state.connection.send(idle()).await {
                error!(error = ?e, "failed to start idling after state change");
                let _ = state
                    .events
                    .send(ConnectionEvent::ConnectionClosed(e.into()));
                return Err(());
            }
        }
        Ok(None) => return Err(()), // The connection was closed
        Err(e) => {
            error!(error = ?e, "state change error");
            let _ = state
                .events
                .send(ConnectionEvent::ConnectionClosed(e.into()));
            return Err(());
        }
    }

    Ok(())
}

/*
fn response_to_subsystem(res: Response) -> Result<Option<Subsystem>, ConnectionError> {
    let mut frame = match res.into_single_frame() {
        Ok(f) => f,
        Err(_) => return Err(ConnectionError::InvalidResponse),
    };

    Ok(match frame.get("changed") {
        Some(raw) => Some(Subsystem::from_raw_string(raw)),
        None => {
            if frame.fields_len() != 0 {
                warn!("state change response was not empty but did not contain `changed` key");
            }

            None
        }
    })
}
*/
