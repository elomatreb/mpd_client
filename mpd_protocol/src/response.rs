use std::collections::HashMap;

/// Responses to commands
#[derive(Debug, PartialEq, Eq)]
pub enum Response {
    /// Empty response (plain success)
    Empty,
    /// Simple (key-value) response
    Simple(HashMap<String, Vec<String>>),
    /// Error response
    Error {
        /// MPD error code, defined in `src/protocol/Ack.hxx`
        error_code: usize,
        /// Index of the command in a command list that caused the error, also
        /// 0 if not in a command list
        command_list_index: usize,
        /// Command that caused the error
        current_command: String,
        /// Message explaining the nature of the error
        message: String,
    },
}
