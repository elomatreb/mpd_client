use mpd_protocol::{AsyncConnection, Response as RawResponse};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::mpsc::{Receiver, UnboundedSender},
    time::timeout,
};
use tracing::{error, span, trace, warn, Instrument, Level};

use std::fmt;
use std::time::Duration;

use super::CommandResponder;
use crate::{
    errors::StateChangeError,
    raw::{RawCommand, RawCommandList},
    state_changes::Subsystem,
};

type StateChangesSender = UnboundedSender<Result<Subsystem, StateChangeError>>;

struct State<C> {
    loop_state: LoopState,
    connection: AsyncConnection<C>,
    commands: Receiver<(RawCommandList, CommandResponder)>,
    state_changes: StateChangesSender,
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
    commands: Receiver<(RawCommandList, CommandResponder)>,
    state_changes: StateChangesSender,
) where
    C: AsyncRead + AsyncWrite + Unpin,
{
    trace!("sending initial idle command");
    if let Err(e) = connection.send(idle()).await {
        error!(error = ?e, "failed to send initial idle command");
        let _ = state_changes.send(Err(e.into()));
    }

    let mut state = State {
        loop_state: LoopState::Idling,
        connection,
        commands,
        state_changes,
    };

    trace!("entering run loop");

    loop {
        let span = span!(Level::TRACE, "iteration", state = ?state.loop_state);

        match run_loop_iteration(state).instrument(span).await {
            Some(new_state) => state = new_state,
            None => break,
        }
    }

    trace!("exited run_loop");
}

/// Time to wait for another command to send before starting the idle loop.
const NEXT_COMMAND_IDLE_TIMEOUT: Duration = Duration::from_millis(100);

async fn run_loop_iteration<C>(mut state: State<C>) -> Option<State<C>>
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
                    match response {
                        Ok(Some(res)) => {
                            if let Some(state_change) = response_to_subsystem(res).transpose() {
                                trace!(?state_change);
                                let _ = state.state_changes.send(state_change);
                            }

                            if let Err(e) = state.connection.send(idle()).await {
                                error!(error = ?e, "failed to start idling after state change");
                                let _ = state.state_changes.send(Err(e.into()));
                                return None;
                            }
                        }
                        Ok(None) => return None, // The connection was closed
                        Err(e) => {
                            error!(error = ?e, "state change error");
                            let _ = state.state_changes.send(Err(e.into()));
                            return None;
                        }
                    }
                }
                command = state.commands.recv() => {
                    // A command was received or the commands channel was dropped. The latter
                    // is an indicator for us to close the connection.

                    let (command, responder) = command?;
                    trace!(?command, "command received");

                    // Cancel currently ongoing idle
                    if let Err(e) = state.connection.send(cancel_idle()).await {
                        error!(error = ?e, "failed to cancel idle prior to sending command");
                        let _ = responder.send(Err(e.into()));
                        return None;
                    }

                    // Response to CANCEL_IDLE above
                    match state.connection.receive().await {
                        Ok(None) => return None,
                        Ok(Some(res)) => {
                            if let Some(state_change) = response_to_subsystem(res).transpose() {
                                trace!(?state_change);
                                let _ = state.state_changes.send(state_change);
                            }
                        }
                        Err(e) => {
                            error!(error = ?e, "state change error prior to sending command");
                            let _ = responder.send(Err(e.into()));
                            return None;
                        }
                    }

                    // Actually send the command. This sets the state for the next loop
                    // iteration.
                    match state.connection.send_list(command).await {
                        Ok(_) => state.loop_state = LoopState::WaitingForCommandReply(responder),
                        Err(e) => {
                            error!(error = ?e, "failed to send command");
                            let _ = responder.send(Err(e.into()));
                            return None;
                        }
                    }

                    trace!("command sent successfully");
                }
            }
        }
        LoopState::WaitingForCommandReply(responder) => {
            // We're waiting for the response to the command associated with `responder`.

            let response = state.connection.receive().await.transpose()?;
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
                            return None;
                        }
                    }
                }
                Ok(None) => return None,
                Err(_) => {
                    trace!("reached next command timeout, idling");

                    // Start idling again
                    state.loop_state = LoopState::Idling;
                    if let Err(e) = state.connection.send(idle()).await {
                        error!(error = ?e, "failed to start idling after receiving command response");
                        let _ = state.state_changes.send(Err(e.into()));
                        return None;
                    }
                }
            }
        }
    }

    Some(state)
}

fn response_to_subsystem(res: RawResponse) -> Result<Option<Subsystem>, StateChangeError> {
    let mut frame = res.single_frame()?;

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
