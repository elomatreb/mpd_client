# `mpd_protocol`

Implementation of the client protocol for [MPD][mpd]. Supports binary responses and command lists.

Primarily consists of an implementation of [Tokio]'s [codec][tokio-codec] subsystem.

## Parser Support

The response parser will understand command lists properly **only** if they are initiated with the `command_list_ok_begin` command.
If the command list is initiated without response separators, all responses will be treated as a single large response which may result in incorrect behavior.

## Installation:

```toml
[dependencies]
mpd_protocol = "0.1"
```

## License

This project is licensed under the GNU General Public License Version 3 or later.

[mpd]: https://musicpd.org
[tokio]: https://tokio.rs
[tokio-codec]: https://docs.rs/tokio-util/0.3.0/tokio_util/codec/index.html
