#![warn(
    rustdoc::broken_intra_doc_links,
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub,
    unused_import_braces,
    unused_qualifications
)]
#![forbid(unsafe_code)]

//! Asynchronous client for [MPD](https://musicpd.org).
//!
//! The [`Client`] type is the primary API.
//!
//! # Crate Features
//!
//! | Feature  | Description                       |
//! |----------|-----------------------------------|
//! | `chrono` | Support for parsing [`Timestamp`] |

#![cfg_attr(docsrs, feature(doc_auto_cfg))]

pub mod client;
pub mod commands;
pub mod filter;
pub mod responses;
pub mod tag;

pub use mpd_protocol as protocol;

pub use self::client::Client;
