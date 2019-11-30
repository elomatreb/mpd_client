# `mpd_protocol`

Implementation of the client protocol for [MPD][mpd].
Supports binary responses and command lists, provided they are initiated with the
`command_list_ok_begin` command.

Consists of a parser for MPD responses (`parser` module), and an implementation of [Tokio][tokio]'s
[`codec`][tokio-codec] subsystem to facilitate asynchronous clients (`codec` module).

## Installation:

```toml
[dependencies]
mpd_protocol = "0.1"
```

## License

This project is licensed under the GNU General Public License Version 3 or later.

[mpd]: https://musicpd.org
[tokio]: https://tokio.rs
[tokio-codec]: https://docs.rs/tokio-util/0.2.0/tokio_util/codec/index.html
