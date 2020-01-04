use std::convert::TryFrom;

use mpd_protocol::command::{Command, CommandError, InvalidCommandReason};

#[test]
fn single() {
    assert_eq!(Command::try_from("status").unwrap().render(), "status\n");

    assert_eq!(Command::new("HELLO WORLD").render(), "hello WORLD\n");

    assert_eq!(Command::new("hello_world").render(), "hello_world\n");

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
        Command::new("command_list_ok_begin").render(),
        "command_list_ok_begin\n"
    );

    assert_eq!(
        Command::new(r#"find "(Artist == \"foo\\'bar\\\"\")""#).render(),
        // The weird indentation below is because you can't use \n in a raw string literal
        r#"find "(Artist == \"foo\\'bar\\\"\")"
"#
    );

    // Unicode is not allowed in the command part
    assert_eq!(
        Command::try_from("stätus").unwrap_err(),
        CommandError {
            reason: InvalidCommandReason::InvalidCharacter(2, 'ä'),
            list_at: None,
        }
    );

    // ... but in the arguments it is
    assert_eq!(
        Command::new("hello wörld").render(),
        "hello wörld\n"
    );
}

#[test]
fn command_list() {
    assert_eq!(
        Command::new(&["status", "hello world"][..]).render(),
        "command_list_ok_begin\nstatus\nhello world\ncommand_list_end\n"
    );

    assert_eq!(
        Command::try_from(&[][..]).unwrap_err(),
        CommandError {
            reason: InvalidCommandReason::Empty,
            list_at: None,
        }
    );

    assert_eq!(
        Command::try_from(&["hello", ""][..]).unwrap_err(),
        CommandError {
            reason: InvalidCommandReason::Empty,
            list_at: Some(1),
        }
    );

    assert_eq!(
        Command::try_from(&["hello", "command_list_begin", "mep mep"][..]).unwrap_err(),
        CommandError {
            reason: InvalidCommandReason::NestedCommandList,
            list_at: Some(1),
        }
    );
}
