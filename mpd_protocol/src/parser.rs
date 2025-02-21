//! Parser for MPD responses.

use std::{
    str::{self, FromStr, from_utf8},
    sync::Arc,
};

use nom::{
    IResult,
    branch::alt,
    bytes::streaming::{tag, take, take_until, take_while, take_while1},
    character::{
        is_alphabetic,
        streaming::{char, digit1, newline},
    },
    combinator::{cut, map, map_res, opt},
    sequence::{delimited, separated_pair, terminated, tuple},
};

use crate::response::{Error, ResponseFieldCache};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ParsedComponent {
    EndOfFrame,
    EndOfResponse,
    Error(Error),
    Field { key: Arc<str>, value: String },
    BinaryField { data_length: usize },
}

#[derive(Debug, PartialEq, Eq)]
struct RawError<'raw> {
    code: u64,
    command_index: u64,
    current_command: Option<&'raw str>,
    message: &'raw str,
}

impl ParsedComponent {
    pub(crate) fn parse<'i>(
        i: &'i [u8],
        field_cache: &'_ mut ResponseFieldCache,
    ) -> IResult<&'i [u8], ParsedComponent> {
        alt((
            map(tag("OK\n"), |_| ParsedComponent::EndOfResponse),
            map(tag("list_OK\n"), |_| ParsedComponent::EndOfFrame),
            map(error, |e| ParsedComponent::Error(e.into_owned_error())),
            map(binary_field, |bin| ParsedComponent::BinaryField {
                data_length: bin.len(),
            }),
            map(key_value_field, |(k, v)| ParsedComponent::Field {
                key: field_cache.insert(k),
                value: String::from(v),
            }),
        ))(i)
    }
}

impl RawError<'_> {
    fn into_owned_error(self) -> Error {
        Error {
            code: self.code,
            command_index: self.command_index,
            current_command: self.current_command.map(Box::from),
            message: Box::from(self.message),
        }
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
fn error(i: &[u8]) -> IResult<&[u8], RawError<'_>> {
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
        RawError {
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
        map_res(field_value, from_utf8),
    )(i)
}

fn field_value(i: &[u8]) -> IResult<&[u8], &[u8]> {
    let (i, value) = take_until("\n")(i)?;
    Ok((&i[1..], value))
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
    use nom::{Err as NomErr, Needed};

    use super::*;

    const EMPTY: &[u8] = &[];

    #[test]
    fn greeting() {
        assert_eq!(super::greeting(b"OK MPD 0.21.11\n"), Ok((EMPTY, "0.21.11")));
        assert!(
            super::greeting(b"OK MPD 0.21.11")
                .unwrap_err()
                .is_incomplete()
        );
    }

    #[test]
    fn end_markers() {
        let keys = &mut ResponseFieldCache::new();

        assert_eq!(
            ParsedComponent::parse(b"OK\n", keys),
            Ok((EMPTY, ParsedComponent::EndOfResponse))
        );

        assert_eq!(
            ParsedComponent::parse(b"OK", keys),
            Err(NomErr::Incomplete(Needed::new(1)))
        );

        assert_eq!(
            ParsedComponent::parse(b"list_OK\n", keys),
            Ok((EMPTY, ParsedComponent::EndOfFrame))
        );

        assert_eq!(
            ParsedComponent::parse(b"list_OK", keys),
            Err(NomErr::Incomplete(Needed::new(1)))
        );
    }

    #[test]
    fn parse_error() {
        let keys = &mut ResponseFieldCache::new();
        let no_command = b"ACK [5@0] {} unknown command \"foo\"\n";
        let with_command = b"ACK [2@0] {random} Boolean (0/1) expected: foo\n";

        assert_eq!(
            ParsedComponent::parse(no_command, keys),
            Ok((
                EMPTY,
                ParsedComponent::Error(Error {
                    code: 5,
                    command_index: 0,
                    current_command: None,
                    message: Box::from("unknown command \"foo\""),
                })
            ))
        );

        assert_eq!(
            ParsedComponent::parse(with_command, keys),
            Ok((
                EMPTY,
                ParsedComponent::Error(Error {
                    code: 2,
                    command_index: 0,
                    current_command: Some(Box::from("random")),
                    message: Box::from("Boolean (0/1) expected: foo"),
                }),
            ))
        );
    }

    #[test]
    fn field() {
        let keys = &mut ResponseFieldCache::new();

        assert_eq!(
            ParsedComponent::parse(b"foo: OK\n", keys),
            Ok((
                EMPTY,
                ParsedComponent::Field {
                    key: Arc::from("foo"),
                    value: String::from("OK"),
                }
            ))
        );

        assert_eq!(
            ParsedComponent::parse(b"foo_bar: hello world list_OK\n", keys),
            Ok((
                EMPTY,
                ParsedComponent::Field {
                    key: Arc::from("foo_bar"),
                    value: String::from("hello world list_OK"),
                }
            ))
        );

        assert!(
            ParsedComponent::parse(b"asdf: fooo", keys)
                .unwrap_err()
                .is_incomplete()
        );
    }

    #[test]
    fn binary_field() {
        let keys = &mut ResponseFieldCache::new();

        assert_eq!(
            ParsedComponent::parse(b"binary: 6\nFOOBAR\n", keys),
            Ok((EMPTY, ParsedComponent::BinaryField { data_length: 6 }))
        );

        assert_eq!(
            ParsedComponent::parse(b"binary: 6\nF", keys),
            Err(NomErr::Incomplete(Needed::new(5)))
        );

        assert_eq!(
            ParsedComponent::parse(b"binary: 12\n", keys),
            Err(NomErr::Incomplete(Needed::new(12)))
        );
    }
}
