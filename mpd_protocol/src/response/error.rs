//! A response to a command indicating an error.

/// A response to a command indicating an error.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Error {
    /// Error code. See [the MPD source][mpd-error-def] for a list of of possible values.
    ///
    /// [mpd-error-def]: https://github.com/MusicPlayerDaemon/MPD/blob/master/src/protocol/Ack.hxx#L30
    pub code: u64,
    /// Index of command in a command list that caused this error. 0 when not in a command list.
    pub command_index: u64,
    /// Command that returned the error, if applicable.
    pub current_command: Option<String>,
    /// Message describing the error.
    pub message: String,
}
