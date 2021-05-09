//! Tools for handling state-change events emitted by MPD.

use futures::stream::Stream;
use tokio::sync::mpsc::UnboundedReceiver;

use std::pin::Pin;
use std::task::{Context, Poll};

pub use crate::errors::StateChangeError;

/// Stream of state change events.
///
/// This is emitted by MPD during the client idle loops. You can use this to keep local state such
/// as the current volume or queue in sync with MPD. The stream ending (yielding `None`) indicates
/// that the MPD server closed the connection, after which no more events will be emitted and
/// attempting to send a command will return an error.
///
/// If you don't care about these, you can just drop this receiver.
///
/// ```no_run
/// use mpd_client::Client;
/// use futures::stream::StreamExt; // For .next()
///
/// async fn print_songs() -> Result<(), Box<dyn std::error::Error>> {
///     let (_client, mut state_changes) = Client::connect_to("localhost:6600").await?;
///
///     while let Some(Ok(state_change)) = state_changes.next().await {
///         println!("state change: {:?}", state_change);
///     }
///
///     Ok(())
/// }
/// ```
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
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Subsystem {
    Database,
    Message,
    Mixer,
    Options,
    Output,
    Partition,
    Player,
    /// Called `playlist` in the protocol.
    Queue,
    Sticker,
    StoredPlaylist,
    Subscription,
    Update,
    Neighbor,
    Mount,

    /// Catch-all variant used when the above variants do not match. Includes the raw subsystem
    /// from the MPD response.
    Other(Box<str>),
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
            "playlist" => Subsystem::Queue,
            "sticker" => Subsystem::Sticker,
            "stored_playlist" => Subsystem::StoredPlaylist,
            "subscription" => Subsystem::Subscription,
            "update" => Subsystem::Update,
            "neighbor" => Subsystem::Neighbor,
            "mount" => Subsystem::Mount,
            _ => Subsystem::Other(raw.into()),
        }
    }
}
