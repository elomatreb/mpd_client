# 1.0.0 (2022-08-27)

 - Redesign and simplify the `Argument` trait
 - Make `CommandError` type opaque
 - API changes:
   - Rename `Frame::get_binary` to `Frame::take_binary`
   - Rename `Response::single_frame` to `Response::into_single_frame`
   - Remove `Response` root reexport

# 0.13.0 (2021-12-09)

 - Redesign connection interface.
   - Instead of standalone functions for the synchronous API and a `Codec` implementation for the asynchronous API, connections are now represented as structs with either synchronous or asynchronous methods (`Connection` and `AsyncConnection`).

     As a result, the asynchronous API no longer consists of a `Sink` for commands and a corresponding `Stream` of responses, but individual methods that either write commands or read a response.

# 0.12.1 (2021-05-13)

 - No external changes (only doc fixes)

# 0.12.0 (2021-05-13)

 - Make async functionality optional by moving it behind the default-off `async` feature flag. Without it, the `tokio` dependencies are removed.
 - Rename error type from `MpdCodecError` to `MpdProtocolError` to reflect the above change.
   - Remove raw message contents from `InvalidMessage` error variant.
 - API changes:
   - Remove `Response::new()` and `Response::empty()` methods
   - Rename `Response::len()` to `Response::successful_frames()`
   - Remove `Frame::empty()`
   - Add `DoubleEndedIterator` implementations for response frame iterators
 - Internal improvements.

# 0.11.0 (2021-01-01)

 - Update to `tokio` 1.0.

# 0.10.1 (2020-11-20)

 - Update to `nom` 6.

# 0.10.0 (2020-11-02)

 - Update `tokio-util` and `bytes` crates.

# 0.9.0 (2020-10-23)

 - Update to tokio 0.3
 - Provide basic functions for sending and receiving using synchronous IO
 - Don't depend on nom features we don't actually use
 - Reword error messages to follow API guidelines

# 0.8.1 (2020-08-05)

 - Change license to MIT or Apache 2.0

# 0.8.0 (2020-08-02)

 - Rewritten parser that incrementally builds up a response
 - Explicit connection method that creates a codec instead of handling the handshake internally
 - Overhauled Frame APIs
 - Removed `command_list` macro
 - Many smaller changes
