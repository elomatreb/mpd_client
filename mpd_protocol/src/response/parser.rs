//! [`nom`](https://github.com/Geal/nom)-based parsers for MPD responses.

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
/// Parsed from raw data using [`response`](fn.response.html).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Response<'a> {
    Success {
        fields: Vec<(&'a str, &'a str)>,
        binary: Option<&'a [u8]>,
    },
    Error {
        code: u64,
        command_index: u64,
        current_command: Option<&'a str>,
        message: &'a str,
    },
}

/// Parse a [`Greeting`](struct.Greeting.html) line.
///
/// ```
/// use mpd_protocol::response::parser::{Greeting, greeting};
///
/// let g = Greeting { version: "0.21.11" };
/// assert_eq!(greeting(b"OK MPD 0.21.11\n"), Ok((&b""[..], g)));
/// ```
pub fn greeting(i: &[u8]) -> IResult<&[u8], Greeting> {
    map(
        delimited(tag("OK MPD "), version_number, char('\n')),
        |version| Greeting { version },
    )(i)
}

/// Parse a complete response, resulting in one or more frames if succesful.
///
/// ```
/// use mpd_protocol::response::parser::{Response, response};
///
/// assert_eq!(
///     response(b"foo: bar\nOK\n"),
///     Ok(([].as_ref(), vec![Response::Success { fields: vec![("foo", "bar")], binary: None }]))
/// );
/// ```
pub fn response(i: &[u8]) -> IResult<&[u8], Vec<Response>> {
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
fn error(i: &[u8]) -> IResult<&[u8], Response> {
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
fn single_response_frame(i: &[u8]) -> IResult<&[u8], Response> {
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
fn command_list(i: &[u8]) -> IResult<&[u8], Vec<Response>> {
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
fn command_list_terminator(i: &[u8]) -> IResult<&[u8], Option<Response>> {
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
    fn empty_response() {
        assert_eq!(
            super::response(b"OK\n"),
            Ok((
                EMPTY,
                vec![Response::Success {
                    fields: Vec::new(),
                    binary: None
                }]
            ))
        );
    }

    #[test]
    fn simple_response() {
        assert_eq!(
            super::response(b"foo: bar\nfoo: baz\nmep: asdf\nOK\n"),
            Ok((
                EMPTY,
                vec![Response::Success {
                    fields: vec![("foo", "bar"), ("foo", "baz"), ("mep", "asdf")],
                    binary: None,
                }],
            ))
        );
    }

    #[test]
    fn binary_response() {
        assert_eq!(
            super::response(b"foo: bar\nbinary: 14\nBINARY_OK\n_MEP\nOK\n"),
            Ok((
                EMPTY,
                vec![Response::Success {
                    fields: vec![("foo", "bar")],
                    binary: Some(b"BINARY_OK\n_MEP"),
                }],
            ))
        );
    }

    #[test]
    fn error_response() {
        assert_eq!(
            super::response(b"ACK [5@0] {} unknown command \"foo\"\n"),
            Ok((
                EMPTY,
                vec![Response::Error {
                    code: 5,
                    command_index: 0,
                    current_command: None,
                    message: "unknown command \"foo\""
                }]
            ))
        );
    }

    #[test]
    fn successful_command_list() {
        assert_eq!(
            super::response(b"hello: world\nlist_OK\nlist_OK\nOK\n"),
            Ok((
                EMPTY,
                vec![
                    Response::Success {
                        fields: vec![("hello", "world")],
                        binary: None,
                    },
                    Response::Success {
                        fields: Vec::new(),
                        binary: None,
                    }
                ]
            ))
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

    #[test]
    fn incomplete_simple_response() {
        let msg = "foo: bar\nOK".as_bytes(); // Note missing final newline
        assert!(super::response(msg).unwrap_err().is_incomplete());
    }

    #[test]
    fn incomplete_binary_response() {
        let msg = "binary: 100\nfoo: bar\n".as_bytes();
        let r = super::response(msg);

        assert!(r.unwrap_err().is_incomplete());
    }

    #[test]
    fn incomplete_complete_response() {
        let msg = "foo: bar\nlist_OK\n".as_bytes();
        let r = super::response(msg);

        assert!(r.unwrap_err().is_incomplete());
    }
}
