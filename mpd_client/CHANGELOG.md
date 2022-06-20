# 0.7.5

 - Add `Ping` command.
 - Add commands for managing enabled metadata tags (`TagTypes` and `EnabledTagTypes`).

# 0.7.4 (2022-06-04)

 - Fix `ListAllIn` error when response includes playlist objects.

# 0.7.3 (2022-03-15)

 - Fix `List::group_by` generating invalid commands when used (due to missing keyword).

# 0.7.2 (2022-02-20)

 - Add a utility method for connecting with an *optional* password (`Client::connect_with_password_opt`).
 - Require tokio 0.16.1.

# 0.7.1 (2021-12-10)

 - Fix panic when parsing a `Song` response that contains negative or invalid duration values.

# 0.7.0 (2021-12-09)

 - Response types for typed commands are now marked as `#[non_exhaustive]` where reasonable.

   This will allow future fields added to MPD to be added to the responses without breaking compatibility. As a result, the `Password` command and the `Client` method have been removed.
 - Rework connection password handling.

   Passwords are now specified on the initial connect and sent immediately after. This avoids issues where the `idle` command of the background task is sent before the password, resulting in spurious "permission denied" errors with restrictively configured MPD servers ([#10](https://github.com/elomatreb/mpd_client/issues/10)).
 - Added new features introduced in version 0.23 of MPD:
   - New tags (`ComposerSort`, `Ensemble`, `Location`, `Movement`, `MovementNumber`)
   - New position options for certain commands (`Add`, `AddToPlaylist`, `RemoveFromPlaylist`)
   - Rework `Move` command to use a builder
 - Command types are no longer `Copy` if they have private fields (to aid in forward compatibility).
 - The `Tag` enum now has forward-compatible equality based on the string representation. If a new variant is added, it will be equal to the `Other(_)` variant containing the same string.
 - Updated `mpd_protocol` dependency.

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
