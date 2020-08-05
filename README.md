# `mpd_client`

Asynchronous client for [MPD](https://musicpd.org).

## Features

 - Asynchronous, using [tokio](https://tokio.rs).
 - Supports protocol version 0.21 and binary responses (e.g. for loading album art).
 - Typed command API that automatically deals with converting the response into proper Rust structs.
 - API for programmatically generating filter expressions without string wrangling.

## Example

See the `examples` directory for a demo of using printing the currently playing song whenever it changes.

## License

Licensed under either of

 * Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
