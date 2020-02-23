#![warn(
    missing_copy_implementations,
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub,
    unused_import_braces,
    unused_qualifications
)]
#![forbid(unsafe_code)]

//! User-friendly async client for [MPD](https://musicpd.org).

pub mod client;
pub mod util;

pub use client::Client;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
