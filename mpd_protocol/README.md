# `mpd_protocol`

Implementation of the client protocol for [MPD].

## Features

 - Protocol support including binary responses and command lists
 - Support for traditional blocking IO as well as asynchronous IO (through [Tokio], requires the `async` feature flag)
 - Utilities for assembling commands and escaping arguments

## License

Licensed under either of

 - Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 - MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

#### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

[MPD]: https://musicpd.org
[Tokio]: https://tokio.rs
