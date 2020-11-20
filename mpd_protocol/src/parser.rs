//! Parsers for MPD responses.
//!
//! # Feature support
//!
//! The parser supports binary fields as the last field in a given message, and command lists if
//! they were initiated with the `command_list_ok_begin` command.
//!
//! If the command list was initiated with the regular `command_list_begin` command, the individual
//! responses are not separated from each other, which causes the responses to be treated as a
//! single large one.

use nom::{
    branch::alt,
    bytes::streaming::{tag, take, take_while, take_while1},
    character::{
        is_alphabetic,
        streaming::{char, digit1, newline},
    },
    combinator::{cut, map, map_res, opt},
    sequence::{delimited, separated_pair, terminated, tuple},
    IResult,
};

use std::str::{self, from_utf8, FromStr};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ParsedComponent<'raw> {
    EndOfFrame,
    EndOfResponse,
    Error(Error<'raw>),
    Field { key: &'raw str, value: &'raw str },
    BinaryField(&'raw [u8]),
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Error<'raw> {
    pub(crate) code: u64,
    pub(crate) command_index: u64,
    pub(crate) current_command: Option<&'raw str>,
    pub(crate) message: &'raw str,
}

impl<'raw> ParsedComponent<'raw> {
    pub(crate) fn parse(i: &'raw [u8]) -> IResult<&[u8], ParsedComponent<'_>> {
        alt((
            map(tag("OK\n"), |_| ParsedComponent::EndOfResponse),
            map(tag("list_OK\n"), |_| ParsedComponent::EndOfFrame),
            map(error, ParsedComponent::Error),
            map(binary_field, |bin| ParsedComponent::BinaryField(bin)),
            map(key_value_field, |(key, value)| ParsedComponent::Field {
                key,
                value,
            }),
        ))(i)
    }
}

/// Recognize a server greeting, returning the protocol version.
pub(crate) fn greeting(i: &[u8]) -> IResult<&[u8], &str> {
    delimited(
        tag("OK MPD "),
        map_res(take_while1(|c| c != b'\n'), from_utf8),
        newline,
    )(i)
}

/// Recognize and parse an unsigned ASCII-encoded number
fn number<O: FromStr>(i: &[u8]) -> IResult<&[u8], O> {
    map_res(map_res(digit1, from_utf8), str::parse)(i)
}

/// Parse an error response.
fn error(i: &[u8]) -> IResult<&[u8], Error<'_>> {
    let (remaining, ((code, index), command, message)) = delimited(
        tag("ACK "),
        tuple((
            terminated(error_code_and_index, char(' ')),
            terminated(error_current_command, char(' ')),
            map_res(take_while(|b| b != b'\n'), from_utf8),
        )),
        newline,
    )(i)?;

    Ok((
        remaining,
        Error {
            code,
            message,
            command_index: index,
            current_command: command,
        },
    ))
}

/// Recognize `[<error code>@<command index>]`.
fn error_code_and_index(i: &[u8]) -> IResult<&[u8], (u64, u64)> {
    delimited(
        char('['),
        separated_pair(number, char('@'), number),
        char(']'),
    )(i)
}

/// Recognize the current command in an error, `None` if empty.
fn error_current_command(i: &[u8]) -> IResult<&[u8], Option<&str>> {
    delimited(
        char('{'),
        opt(map_res(
            take_while1(|b| is_alphabetic(b) || b == b'_'),
            from_utf8,
        )),
        char('}'),
    )(i)
}

/// Recognize a single key-value pair
fn key_value_field(i: &[u8]) -> IResult<&[u8], (&str, &str)> {
    separated_pair(
        map_res(
            take_while1(|b| is_alphabetic(b) || b == b'_' || b == b'-'),
            from_utf8,
        ),
        tag(": "),
        map_res(terminated(take_while(|b| b != b'\n'), newline), from_utf8),
    )(i)
}

/// Recognize the header of a binary section
fn binary_prefix(i: &[u8]) -> IResult<&[u8], usize> {
    delimited(tag("binary: "), number, newline)(i)
}

/// Recognize a binary field
fn binary_field(i: &[u8]) -> IResult<&[u8], &[u8]> {
    let (i, length) = binary_prefix(i)?;

    cut(terminated(take(length), newline))(i)
}

#[cfg(test)]
mod test {
    use super::{Error, ParsedComponent};
    use nom::{Err as NomErr, Needed};

    const EMPTY: &[u8] = &[];

    #[test]
    fn greeting() {
        assert_eq!(super::greeting(b"OK MPD 0.21.11\n"), Ok((EMPTY, "0.21.11")));
        assert!(super::greeting(b"OK MPD 0.21.11")
            .unwrap_err()
            .is_incomplete());
    }

    #[test]
    fn end_markers() {
        assert_eq!(
            ParsedComponent::parse(b"OK\n"),
            Ok((EMPTY, ParsedComponent::EndOfResponse))
        );

        assert_eq!(
            ParsedComponent::parse(b"OK"),
            Err(NomErr::Incomplete(Needed::new(1)))
        );

        assert_eq!(
            ParsedComponent::parse(b"list_OK\n"),
            Ok((EMPTY, ParsedComponent::EndOfFrame))
        );

        assert_eq!(
            ParsedComponent::parse(b"list_OK"),
            Err(NomErr::Incomplete(Needed::new(1)))
        );
    }

    #[test]
    fn error() {
        let no_command = b"ACK [5@0] {} unknown command \"foo\"\n";
        let with_command = b"ACK [2@0] {random} Boolean (0/1) expected: foo\n";

        assert_eq!(
            ParsedComponent::parse(no_command),
            Ok((
                EMPTY,
                ParsedComponent::Error(Error {
                    code: 5,
                    command_index: 0,
                    current_command: None,
                    message: "unknown command \"foo\""
                })
            ))
        );

        assert_eq!(
            ParsedComponent::parse(with_command),
            Ok((
                EMPTY,
                ParsedComponent::Error(Error {
                    code: 2,
                    command_index: 0,
                    current_command: Some("random"),
                    message: "Boolean (0/1) expected: foo",
                })
            ))
        );
    }

    #[test]
    fn field() {
        assert_eq!(
            ParsedComponent::parse(b"foo: OK\n"),
            Ok((
                EMPTY,
                ParsedComponent::Field {
                    key: "foo",
                    value: "OK",
                }
            ))
        );

        assert_eq!(
            ParsedComponent::parse(b"foo_bar: hello world list_OK\n"),
            Ok((
                EMPTY,
                ParsedComponent::Field {
                    key: "foo_bar",
                    value: "hello world list_OK",
                }
            ))
        );

        assert!(ParsedComponent::parse(b"asdf: fooo")
            .unwrap_err()
            .is_incomplete());
    }

    #[test]
    fn binary_field() {
        assert_eq!(
            ParsedComponent::parse(b"binary: 6\nFOOBAR\n"),
            Ok((EMPTY, ParsedComponent::BinaryField(b"FOOBAR")))
        );

        assert_eq!(
            ParsedComponent::parse(b"binary: 6\nF"),
            Err(NomErr::Incomplete(Needed::new(5)))
        );

        assert_eq!(
            ParsedComponent::parse(b"binary: 12\n"),
            Err(NomErr::Incomplete(Needed::new(12)))
        );
    }
}
