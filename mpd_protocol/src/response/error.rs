//! A response to a command indicating an error.

use crate::parser;

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
    pub current_command: Option<Box<str>>,
    /// Message describing the error.
    pub message: Box<str>,
}

impl Error {
    pub(crate) fn from_parsed(parsed: parser::Error<'_>) -> Self {
        Self {
            code: parsed.code,
            command_index: parsed.command_index,
            current_command: parsed.current_command.map(Into::into),
            message: parsed.message.into(),
        }
    }
}
