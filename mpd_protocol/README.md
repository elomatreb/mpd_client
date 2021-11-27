# `mpd_protocol`

Implementation of the client protocol for [MPD][mpd].

See also [`mpd_client`][mpd-client], a crate that uses this library to implement a complete asynchronous client including connection management.

## Features

 - Protocol support including binary responses and command lists
 - Asynchronous IO support through an implementation of [Tokio]'s [codec][tokio-codec] subsystem (requires the `async` feature flag)
 - Utilities for assembling commands and escaping arguments

## Installation

```toml
[dependencies]
mpd_protocol = "0.12"
```

## License

Licensed under either of

 - Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 - MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

#### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

[mpd]: https://musicpd.org
[tokio]: https://tokio.rs
[tokio-codec]: https://docs.rs/tokio-util/0.6.6/tokio_util/codec/index.html
[mpd-client]: https://crates.io/crates/mpd_client
