use std::convert::TryFrom;

use mpd_protocol::command::{Command, CommandError, InvalidCommandReason};

#[test]
fn single() {
    assert_eq!(Command::try_from("status").unwrap().render(), "status\n");

    assert_eq!(
        Command::try_from("HELLO WORLD").unwrap().render(),
        "hello WORLD\n"
    );

    assert_eq!(
        Command::try_from("hello_world").unwrap().render(),
        "hello_world\n"
    );

    assert_eq!(
        Command::try_from("").unwrap_err(),
        CommandError {
            reason: InvalidCommandReason::Empty,
            list_at: None
        }
    );

    assert_eq!(
        Command::try_from("hello  ").unwrap_err(),
        CommandError {
            reason: InvalidCommandReason::UnncessaryWhitespace,
            list_at: None,
        }
    );

    assert_eq!(
        Command::try_from("hello$world").unwrap_err(),
        CommandError {
            reason: InvalidCommandReason::InvalidCharacter(5, '$'),
            list_at: None,
        }
    );

    assert_eq!(
        Command::try_from("hello\nworld").unwrap_err(),
        CommandError {
            reason: InvalidCommandReason::InvalidCharacter(5, '\n'),
            list_at: None,
        }
    );

    assert_eq!(
        // this is OK because it's not nesting
        Command::try_from("command_list_ok_begin").unwrap().render(),
        "command_list_ok_begin\n",
    );
}
