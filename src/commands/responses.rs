//! Typed responses to individual commands.

use mpd_protocol::response::Frame;

use std::convert::TryFrom;

/// Errors which occur when attempting to convert a raw `Frame` into the proper typed response.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(missing_copy_implementations)]
pub enum TypedResponseError {

}

/// An empty response, which only indicates success.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Empty;

impl TryFrom<Frame> for Empty {
    type Error = TypedResponseError;

    fn try_from(_: Frame) -> Result<Self, Self::Error> {
        // silently ignore any actually existing fields
        Ok(Self)
    }
}
