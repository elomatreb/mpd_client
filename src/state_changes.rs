//! Tools for handling state-change events emitted by MPD.

use futures::stream::Stream;
use tokio::sync::mpsc::UnboundedReceiver;

use std::pin::Pin;
use std::task::{Context, Poll};

use crate::errors::StateChangeError;

/// Stream of state change events.
///
/// This is emitted by MPD during the client idle loops. You can use this to keep local state such
/// as the current volume or queue in sync with MPD.
///
/// If you don't care about these, you can just drop this receiver.
#[derive(Debug)]
pub struct StateChanges {
    pub(crate) rx: UnboundedReceiver<Result<Subsystem, StateChangeError>>,
}

impl Stream for StateChanges {
    type Item = Result<Subsystem, StateChangeError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // Just delegate for now
        self.rx.poll_recv(cx)
    }
}

/// Subsystems of MPD which can receive state change notifications.
///
/// Derived from [the documentation](https://www.musicpd.org/doc/html/protocol.html#command-idle),
/// but also includes a catch-all to remain forward-compatible.
#[allow(missing_docs)]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Subsystem {
    Database,
    Message,
    Mixer,
    Options,
    Output,
    Partition,
    Player,
    Playlist,
    Sticker,
    StoredPlaylist,
    Subscription,
    Update,

    /// Catch-all variant used when the above variants do not match. Includes the raw subsystem
    /// from the MPD response.
    Other(String),
}

impl Subsystem {
    pub(crate) fn from_raw_string(raw: String) -> Self {
        match raw.as_str() {
            "database" => Subsystem::Database,
            "message" => Subsystem::Message,
            "mixer" => Subsystem::Mixer,
            "options" => Subsystem::Options,
            "output" => Subsystem::Output,
            "partition" => Subsystem::Partition,
            "player" => Subsystem::Player,
            "playlist" => Subsystem::Playlist,
            "sticker" => Subsystem::Sticker,
            "stored_playlist" => Subsystem::StoredPlaylist,
            "subscription" => Subsystem::Subscription,
            "update" => Subsystem::Update,
            _ => Subsystem::Other(raw),
        }
    }
}
