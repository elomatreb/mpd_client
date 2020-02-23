//! Utilities for working with MPD.

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

impl From<String> for Subsystem {
    fn from(raw: String) -> Self {
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
