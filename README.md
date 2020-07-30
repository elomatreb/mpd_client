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

GPL version 3 or later (see `LICENSE`).
