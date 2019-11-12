use bytes::BytesMut;
use regex::Regex;
use tokio::codec::Decoder;
use tokio::io;

use std::collections::HashMap;
use std::fmt;
use std::str;

use crate::response::Response;

/// Codec for MPD protocol.
#[derive(Debug, Default)]
pub struct Codec {
    examined_up_to: usize,
    parsing_error: bool,
}

impl Codec {
    /// Creates a new Codec
    pub fn new() -> Self {
        Codec::default()
    }
}

impl Decoder for Codec {
    type Item = Response;
    type Error = MpdCodecError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        // Look through the unknown part of our buffer for message terminators
        for window_start in self.examined_up_to..src.len() {
            let window_end = if window_start + 3 <= src.len() {
                window_start + 3
            } else {
                break;
            };

            let window = &src[window_start..window_end];

            if self.examined_up_to == 0 && window == b"ACK" {
                // The following message is an error
                self.parsing_error = true;
            } else if self.parsing_error && &window[2..] == b"\n" {
                // The error message is complete, parse it

                // Our message ends two bytes into the current window
                // Reset state
                let end = self.examined_up_to + 2;
                self.examined_up_to = 0;
                self.parsing_error = false;

                let err = parse_error_line(src.split_to(end))?;
                return Ok(Some(err));
            } else if window == b"OK\n" {
                // A message terminator was found

                if self.examined_up_to == 0 {
                    // The message was just an OK, indicating an empty but successful
                    // response
                    src.advance(3);
                    return Ok(Some(Response::Empty));
                } else if src[window_start - 1] == b'\n' {
                    // The terminator was preceded by a newline, this means the
                    // message is actually complete, split it from buffer
                    // including the terminator bytes
                    let mut msg = src.split_to(window_end);

                    let kv = parse_key_value_response(msg.split_to(msg.len() - 4))?;
                    self.examined_up_to = 0;
                    return Ok(Some(Response::Simple(kv)));
                }

                // If the terminator was not at the start of a buffer or
                // preceeded by a newline, it was part of the message, ignore
                // it
            }

            // Count the windows we examined, so that a possible next call to
            // decode can avoid reexamining them
            self.examined_up_to += 1;
        }

        // Nothing was found
        // Round down to nearest multiple of three in case our buffer was cut
        // off in the middle of a terminator
        self.examined_up_to /= 3;
        Ok(None)
    }
}

fn parse_key_value_response(
    bytes: BytesMut,
) -> Result<HashMap<String, Vec<String>>, MpdCodecError> {
    let mut map = HashMap::new();
    let string = str::from_utf8(&bytes).unwrap();

    for line in string.split('\n') {
        let i = line.trim().find(':');
        if i.is_none() || i == Some(line.len() - 1) {
            return Err(MpdCodecError::InvalidKeyValueSequence);
        }

        let (key, value) = line.split_at(i.unwrap());
        let value = value[1..].trim();

        map.entry(key.to_owned())
            .or_insert_with(Vec::new)
            .push(value.to_owned());
    }

    Ok(map)
}

fn parse_error_line(bytes: BytesMut) -> Result<Response, MpdCodecError> {
    lazy_static::lazy_static! {
        static ref ERROR_REGEX: Regex = {
            Regex::new(r"^ACK \[(\d+)@(\d+)\] \{(\w+?)\} (.+)$").unwrap()
        };
    }

    if let Some(captures) = ERROR_REGEX.captures(str::from_utf8(&bytes).unwrap()) {
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

/// An error occured in a response
#[derive(Debug)]
pub enum MpdCodecError {
    /// IO error occured
    Io(io::Error),
    /// A line wasn't a "key: value"
    InvalidKeyValueSequence,
    /// A line started like an error but wasn't correctly formatted
    InvalidErrorMessage,
}

impl fmt::Display for MpdCodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MpdCodecError::InvalidKeyValueSequence => {
                write!(f, "response contained invalid key-sequence")
            }
            MpdCodecError::InvalidErrorMessage => {
                write!(f, "response contained invalid error message")
            }
            MpdCodecError::Io(e) => write!(f, "{}", e),
        }
    }
}

impl From<io::Error> for MpdCodecError {
    fn from(e: io::Error) -> Self {
        MpdCodecError::Io(e)
    }
}

impl std::error::Error for MpdCodecError {}
