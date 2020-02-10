use std::convert::TryFrom;

use mpd_protocol::command::{Command, CommandError, InvalidCommandReason};
use mpd_protocol::Filter;

#[test]
fn try_from() {
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
        Command::try_from(r#"find "(Artist == \"foo\\'bar\\\"\")""#)
            .unwrap()
            .render(),
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
        Command::try_from("hello wörld").unwrap().render(),
        "hello wörld\n"
    );
}

#[test]
fn builder() {
    assert_eq!(Command::build("status").unwrap().render(), "status\n");

    assert_eq!(
        Command::build("pause").argument("1").unwrap().render(),
        "pause 1\n"
    );

    assert_eq!(
        Command::build("hello")
            .argument("foo bar")
            .unwrap()
            .render(),
        "hello \"foo bar\"\n"
    );

    assert_eq!(
        Command::build("hello")
            .argument("foo's bar\"")
            .unwrap()
            .render(),
        "hello \"foo\\'s bar\\\"\"\n"
    );

    assert_eq!(
        Command::build("status")
            .command("currentsong")
            .unwrap()
            .render(),
        "command_list_ok_begin\nstatus\ncurrentsong\ncommand_list_end\n"
    );

    assert_eq!(
        Command::build(" hello").finish().unwrap_err(),
        CommandError {
            reason: InvalidCommandReason::UnncessaryWhitespace,
            list_at: None,
        }
    );

    assert_eq!(
        Command::build("status")
            .command(" currentsong")
            .finish()
            .unwrap_err(),
        CommandError {
            reason: InvalidCommandReason::UnncessaryWhitespace,
            list_at: Some(1),
        }
    );
}

#[test]
fn filter() {
    assert_eq!(
        Command::build("find")
            .filter(Filter::equal("album", "hello world"))
            .unwrap()
            .render(),
        "find \"(album == \\\"hello world\\\")\"\n"
    );
}
