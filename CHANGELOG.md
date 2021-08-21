# 0.6.1 (2021-08-21)

 - Add a limited degree of backwards compatibility for protocol versions older than 0.20 ([#9](https://github.com/elomatreb/mpd_client/pull/9), thanks to D3fus).
   Specifically, support parsing song durations with fallback to deprecated fields.
   **NOTE**: Other features still do **not** support these old protocols, notably the filter expressions used by certain commands.
 - Add a utility method for retrieving MPD subsystem protocol names.
 - Fix missing `Command` impl for `SetBinaryLimit` command.

# 0.6.0 (2021-05-17)

 - Update `mpd_protocol`
 - Add `Client::album_art` method for loading album art
 - Add new MPD subsystems
 - API changes:
   - Remove `Client::connect_to` and `Client::connect_unix` methods
   - Rename `Command::to_command` to `Command::into_command`

# 0.5.1 (2021-04-28)

 - Fix error when parsing list of songs response containing modified timestamps for directories ([#7](https://github.com/elomatreb/mpd_client/issues/7))

# 0.5.0 (2021-01-01)

 - Update to `tokio` 1.0.

# 0.4.0 (2020-11-06)

 - Add typed commands and command list API
 - Update to tokio 0.3
 - Adapt to MPD 0.22 versions
