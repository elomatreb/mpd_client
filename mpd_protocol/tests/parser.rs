use mpd_protocol::parser::{response, Response};

static EMPTY: &[u8] = &[];

#[test]
fn incomplete_simple_response() {
    let msg = "foo: bar\nOK".as_bytes(); // Note missing final newline
    assert!(response(msg).unwrap_err().is_incomplete());
}

#[test]
fn incomplete_binary_response() {
    let msg = "binary: 100\nfoo: bar\n".as_bytes();
    let r = response(msg);

    assert!(r.unwrap_err().is_incomplete());
}

#[test]
fn incomplete_complete_response() {
    let msg = "foo: bar\nlist_OK\n".as_bytes();
    let r = response(msg);

    assert!(r.unwrap_err().is_incomplete());
}

#[test]
fn empty_response() {
    assert_eq!(
        response(b"OK\n"),
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
        response(b"foo: bar\nfoo: baz\nmep: asdf\nOK\n"),
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
fn simple_response_with_terminator_in_values() {
    assert_eq!(
        response(b"hello: world\nfoo: OK\nbar: 1234\nOK\n"),
        Ok((
            EMPTY,
            vec![Response::Success {
                fields: vec![("hello", "world"), ("foo", "OK"), ("bar", "1234")],
                binary: None,
            }],
        ))
    );
}

#[test]
fn binary_response() {
    assert_eq!(
        response(b"foo: bar\nbinary: 14\nBINARY_OK\n_MEP\nOK\n"),
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
        response(b"ACK [5@0] {} unknown command \"foo\"\n"),
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
        response(b"hello: world\nlist_OK\nlist_OK\nOK\n"),
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
fn command_list_with_error() {
    assert_eq!(
        response(b"foo: bar\nlist_OK\nACK [5@0] {} unknown command \"foo\"\n"),
        Ok((
            EMPTY,
            vec![
                Response::Success {
                    fields: vec![("foo", "bar")],
                    binary: None,
                },
                Response::Error {
                    code: 5,
                    command_index: 0,
                    current_command: None,
                    message: "unknown command \"foo\"",
                }
            ]
        ))
    );
}
