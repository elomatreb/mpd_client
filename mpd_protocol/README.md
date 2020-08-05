# `mpd_protocol`

Implementation of the client protocol for [MPD][mpd]. Supports binary responses and command lists.

Primarily consists of an implementation of [Tokio]'s [codec][tokio-codec] subsystem.

See also [`mpd_client`][mpd-client], a crate that uses this library to implement a complete asynchronous client including connection management.

## Parser Support

The response parser will understand command lists properly **only** if they are initiated with the `command_list_ok_begin` command.
If the command list is initiated without response separators, all responses will be treated as a single large response which may result in incorrect behavior.

## Installation:

```toml
[dependencies]
mpd_protocol = "0.8"
```

## License

Licensed under either of

 * Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

[mpd]: https://musicpd.org
[tokio]: https://tokio.rs
[tokio-codec]: https://docs.rs/tokio-util/0.3.0/tokio_util/codec/index.html
[mpd-client]: https://crates.io/crates/mpd_client
