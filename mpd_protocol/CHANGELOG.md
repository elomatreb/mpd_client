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
