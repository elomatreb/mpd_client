# 1.0.0 (2022-08-27)

 - Redesign the `Command` and `CommandList` traits
   - Remove trait seal, you can add your own impls now
   - Remove the `Response` trait, response creation is now handled by a method on the respective trait
   - Add public functions for constructing `TypedResponseError`s
 - Reorganize crate modules
   - Commands now live in their own top-level module
   - Error types now live in the modules where they are used
 - Make `chrono` dependency optional
 - Rename `StateChanges` to `ConnectionEvents` and return an enum of possible events.
 - Redesign commands to take references to their arguments where necessary instead of taking ownership.
 - Add commands for managing song stickers ([#14](https://github.com/elomatreb/mpd_client/pull/14), thanks to JakeStanger).
 - Add `Count` command (proposed by pborzenkov in [#15](https://github.com/elomatreb/mpd_client/pull/15)).
 - Reimplement `List` command to support type-safe grouping.
 - Bug fixes:
   - Missing `CommandList` impl for tuples of size 4
   - Missing argument rendering on `GetPlaylist` commnad
 - Other API changes:
   - Clean up crate reexports. Now simply reexports the entire `mpd_protocol` crate as `protocol`.
   - Add `Client::is_connection_closed`
   - `Status` response: Don't suppress the `default` partition name
   - `AlbumArt` response: Expose returned raw data as `BytesMut`
   - `Client::album_art`: Return loaded data as `BytesMut`

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
