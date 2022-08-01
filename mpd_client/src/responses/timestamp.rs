#[cfg(feature = "chrono")]
use chrono::{DateTime, FixedOffset};

use crate::responses::{FromFieldValue, TypedResponseError};

/// A timestamp, used for modification times.
///
/// This is a newtype wrapper to allow the optional use of the `chrono` library.
#[derive(Clone, Debug, Eq)]
pub struct Timestamp {
    raw: String,
    #[cfg(feature = "chrono")]
    chrono: DateTime<FixedOffset>,
}

impl Timestamp {
    /// Returns the timestamp string as it was returned by the server.
    pub fn raw(&self) -> &str {
        &self.raw
    }

    /// Returns the timestamp string as it was returned by the server.
    #[cfg(feature = "chrono")]
    pub fn chrono_datetime(&self) -> DateTime<FixedOffset> {
        self.chrono
    }
}

impl PartialEq for Timestamp {
    fn eq(&self, other: &Self) -> bool {
        self.raw == other.raw
    }
}

#[cfg(feature = "chrono")]
impl PartialEq<DateTime<FixedOffset>> for Timestamp {
    fn eq(&self, other: &DateTime<FixedOffset>) -> bool {
        &self.chrono == other
    }
}

impl PartialOrd for Timestamp {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.raw.partial_cmp(&other.raw)
    }
}

impl Ord for Timestamp {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.raw.cmp(&other.raw)
    }
}

#[cfg(feature = "chrono")]
impl PartialOrd<DateTime<FixedOffset>> for Timestamp {
    fn partial_cmp(&self, other: &DateTime<FixedOffset>) -> Option<std::cmp::Ordering> {
        self.chrono.partial_cmp(other)
    }
}

impl FromFieldValue for Timestamp {
    #[cfg_attr(not(feature = "chrono"), allow(unused_variables))]
    fn from_value(v: String, field: &str) -> Result<Self, TypedResponseError> {
        #[cfg(feature = "chrono")]
        let chrono = match DateTime::parse_from_rfc3339(&v) {
            Ok(v) => v,
            Err(e) => return Err(TypedResponseError::invalid_value(field, v).source(e)),
        };

        Ok(Self {
            raw: v,
            #[cfg(feature = "chrono")]
            chrono,
        })
    }
}
