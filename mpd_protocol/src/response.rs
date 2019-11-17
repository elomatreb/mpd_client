use bytes::BytesMut;
use lazy_static::lazy_static;
use regex::Regex;

use std::collections::HashMap;
use std::str;

use crate::MpdCodecError;

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

impl Response {
    /// Parse a "simple" response (not an error or just a simple OK response)
    pub(crate) fn parse_simple(bytes: BytesMut) -> Result<Self, MpdCodecError> {
        let mut map = HashMap::new();
        let mut examined = 0;

        loop {
            // Look for next newline
            let newline = bytes[examined..]
                .iter()
                .position(|b| *b == b'\n')
                // If no newline was found, look until end of buffer
                .unwrap_or_else(|| bytes.len() - examined);

            let line = &bytes[examined..examined + newline];
            let (key, value) = parse_line(line)?;

            // Insert results into map
            map.entry(key.to_owned())
                .or_insert_with(Vec::new)
                .push(value.to_owned());

            // Start with the remaining buffer next time
            examined += newline + 1;

            if examined >= bytes.len() - 1 {
                // We examined the entire buffer
                break;
            }
        }

        Ok(Response::Simple(map))
    }

    /// Parse an error response
    pub(crate) fn parse_error(bytes: BytesMut) -> Result<Self, MpdCodecError> {
        lazy_static! {
            static ref ERROR_REGEX: Regex =
                { Regex::new(r"^ACK \[(\d+)@(\d+)\] \{(\w*?)\} (.+)$").unwrap() };
        }

        if let Some(captures) = ERROR_REGEX.captures(str::from_utf8(&bytes)?) {
            Ok(Response::Error {
                error_code: captures.get(1).unwrap().as_str().parse().unwrap(),
                command_list_index: captures.get(2).unwrap().as_str().parse().unwrap(),
                current_command: captures.get(3).unwrap().as_str().to_owned(),
                message: captures.get(4).unwrap().as_str().to_owned(),
            })
        } else {
            Err(MpdCodecError::InvalidErrorMessage)
        }
    }
}

fn parse_line(line: &[u8]) -> Result<(&str, &str), MpdCodecError> {
    // A key-value line has to be valid Unicode
    let line = str::from_utf8(line)?;

    // Find ':' separator
    let separator = line.find(':');

    // Return error if the line doesn't contain a separator or the separator is
    // the last character
    if separator.is_none() || separator == Some(line.len() - 1) {
        return Err(MpdCodecError::InvalidKeyValueSequence);
    }

    // The line has the form "<key>: <value>"
    let (key, value) = line.split_at(separator.unwrap());
    // Remove the separator and the leading space from the value
    let value = &value[2..];

    Ok((key, value))
}
