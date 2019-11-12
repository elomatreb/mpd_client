use bytes::BytesMut;
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
        for window in src[self.examined_up_to..].windows(3) {
            if self.examined_up_to == 0 && window == b"ACK" {
                unimplemented!();
            }

            if window == b"OK\n" {
                // Split the buffer after our complete message
                let mut msg = src.split_to(self.examined_up_to + 3);

                if self.examined_up_to == 0 {
                    return Ok(Some(Response::Empty));
                } else {
                    let kv = parse_key_value_response(msg.split_to(msg.len() - 4))?;
                    self.examined_up_to = 0;
                    return Ok(Some(Response::Simple(kv)));
                }
            }

            self.examined_up_to += 1;
        }

        // Nothing was found
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

/// An error occured in a response
#[derive(Debug)]
pub enum MpdCodecError {
    /// IO error occured
    Io(io::Error),
    /// A line wasn't a "key: value"
    InvalidKeyValueSequence,
}

impl fmt::Display for MpdCodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MpdCodecError::InvalidKeyValueSequence => {
                write!(f, "response contained invalid key-sequence")
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
