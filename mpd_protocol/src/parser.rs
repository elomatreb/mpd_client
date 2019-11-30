//! [`nom`](https://github.com/Geal/nom)-based parsers for MPD responses.
//!
//! # Feature support
//!
//! The parser supports binary fields as the last field in a given message, and command lists if
//! they were initiated with the `command_list_ok_begin` command.
//!
//! If the command list was initiated with the regular `command_list_begin` command, the individual
//! responses are not separated from each other, which causes the responses to be treated as a
//! single large one, resulting in an error if any but the last response in the list contain
//! binary fields.

use nom::{
    branch::alt,
    bytes::streaming::{tag, take, take_while, take_while1},
    character::{
        is_alphabetic, is_digit,
        streaming::{char, digit1, newline},
    },
    combinator::{cut, map, map_res, not, opt},
    error::ParseError,
    multi::{many0, many_till},
    sequence::{delimited, pair, separated_pair, terminated, tuple},
    IResult,
};

use std::str::{self, FromStr};

/// Initial message sent by MPD on connect.
///
/// Parsed from raw data using [`greeting`](fn.greeting.html).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Greeting<'a> {
    /// Protocol version reported by MPD.
    pub version: &'a str,
}

/// Complete response, either succesful or an error. Succesful responses may be empty.
///
/// Parsed from raw data using [`response`](fn.response.html). See also the notes about [parser
/// support](index.html#feature-support).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Response<'a> {
    /// Successful response.
    Success {
        /// Key-value pairs. Keys may occur any number of times.
        fields: Vec<(&'a str, &'a str)>,
        /// Binary field.
        binary: Option<&'a [u8]>,
    },
    /// Error response.
    Error {
        /// Error code, as [defined by
        /// MPD](https://github.com/MusicPlayerDaemon/MPD/blob/master/src/protocol/Ack.hxx).
        code: u64,
        /// Index of command that caused this error in a command list.
        command_index: u64,
        /// Command that caused this error, if any.
        current_command: Option<&'a str>,
        /// Message describing the nature of the error.
        message: &'a str,
    },
}

/// Parse a [`Greeting`](struct.Greeting.html) line.
///
/// ```
/// use mpd_protocol::parser::{Greeting, greeting};
///
/// let g = Greeting { version: "0.21.11" };
/// assert_eq!(greeting(b"OK MPD 0.21.11\n"), Ok((&b""[..], g)));
/// ```
pub fn greeting(i: &[u8]) -> IResult<&[u8], Greeting<'_>> {
    map(
        delimited(tag("OK MPD "), version_number, char('\n')),
        |version| Greeting { version },
    )(i)
}

/// Parse a complete response, resulting in one or more frames if succesful.
///
/// See also the notes about [parser support](index.html#feature-support).
///
/// ```
/// use mpd_protocol::parser::{Response, response};
///
/// assert_eq!(
///     response(b"foo: bar\nOK\n"),
///     Ok(([].as_ref(), vec![Response::Success { fields: vec![("foo", "bar")], binary: None }]))
/// );
/// ```
pub fn response(i: &[u8]) -> IResult<&[u8], Vec<Response<'_>>> {
    alt((
        map(error, |r| vec![r]),
        map(terminated(single_response_frame, tag("OK\n")), |r| vec![r]),
        command_list,
    ))(i)
}

/// Apply the given parser and try to interpret its result as UTF-8 encoded bytes.
fn utf8<'a, F, E>(parser: F) -> impl Fn(&'a [u8]) -> nom::IResult<&'a [u8], &str, E>
where
    F: Fn(&'a [u8]) -> nom::IResult<&'a [u8], &'a [u8], E>,
    E: ParseError<&'a [u8]>,
{
    map_res(parser, str::from_utf8)
}

/// Recognize and parse an unsigned ASCII-encoded number
fn number<O: FromStr>(i: &[u8]) -> IResult<&[u8], O> {
    map_res(utf8(digit1), str::parse)(i)
}

/// Recognize a version number.
fn version_number(i: &[u8]) -> IResult<&[u8], &str> {
    // TODO: This accepts version numbers consisting of only dots or ones starting/ending with dots
    utf8(take_while1(|b| is_digit(b) || b == b'.'))(i)
}

/// Parse an error response.
fn error(i: &[u8]) -> IResult<&[u8], Response<'_>> {
    let (remaining, ((code, index), command, message)) = delimited(
        tag("ACK "),
        tuple((
            terminated(error_code_and_index, char(' ')),
            terminated(error_current_command, char(' ')),
            utf8(take_while(|b| b != b'\n')),
        )),
        newline,
    )(i)?;

    Ok((
        remaining,
        Response::Error {
            code,
            message,
            command_index: index,
            current_command: command,
        },
    ))
}

/// Recognize a single succesful response.
fn single_response_frame(i: &[u8]) -> IResult<&[u8], Response<'_>> {
    map(
        pair(many0(key_value_field), opt(binary_field)),
        |(fields, binary)| Response::Success { fields, binary },
    )(i)
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
        opt(utf8(take_while1(|b| is_alphabetic(b) || b == b'_'))),
        char('}'),
    )(i)
}

/// Recognize a single key-value pair
fn key_value_field(i: &[u8]) -> IResult<&[u8], (&str, &str)> {
    // Don't parse a binary field header as a key-value pair
    not(binary_prefix)(i)?;

    separated_pair(
        utf8(take_while1(|b| is_alphabetic(b) || b == b'_' || b == b'-')),
        tag(": "),
        utf8(terminated(take_while1(|b| b != b'\n'), newline)),
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

/// Recognize a command list, consisting of one or more responses and/or an error
fn command_list(i: &[u8]) -> IResult<&[u8], Vec<Response<'_>>> {
    map(
        many_till(
            terminated(single_response_frame, tag("list_OK\n")),
            command_list_terminator,
        ),
        |(mut responses, terminator)| {
            if let Some(error) = terminator {
                responses.push(error);
            }

            responses
        },
    )(i)
}

/// Recognize a terminator for a command list, either the final OK or an error message
fn command_list_terminator(i: &[u8]) -> IResult<&[u8], Option<Response<'_>>> {
    alt((map(tag("OK\n"), |_| None), map(error, Some)))(i)
}

#[cfg(test)]
mod test {
    use super::Response;

    static EMPTY: &[u8] = &[];

    #[test]
    fn version_number() {
        assert_eq!(
            super::version_number(b"0.21.11\n"),
            Ok((&b"\n"[..], "0.21.11"))
        );
    }

    #[test]
    fn error() {
        let no_command = "ACK [5@0] {} unknown command \"foo\"\n";
        let with_command = "ACK [2@0] {random} Boolean (0/1) expected: foo\n";

        assert_eq!(
            super::error(no_command.as_bytes()),
            Ok((
                EMPTY,
                Response::Error {
                    code: 5,
                    command_index: 0,
                    current_command: None,
                    message: "unknown command \"foo\""
                }
            ))
        );

        assert_eq!(
            super::error(with_command.as_bytes()),
            Ok((
                EMPTY,
                Response::Error {
                    code: 2,
                    command_index: 0,
                    current_command: Some("random"),
                    message: "Boolean (0/1) expected: foo",
                }
            ))
        );
    }
}
