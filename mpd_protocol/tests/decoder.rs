use bytes::{BufMut, BytesMut};
use tokio_util::codec::Decoder;

use std::collections::HashMap;

use mpd_protocol::{MpdCodec, Response};

#[test]
fn decoder_handles_greeting() {
    let mut codec = MpdCodec::new();
    let buf = &mut BytesMut::new();

    let msg = "OK MPD 0.21.11\n";
    buf.reserve(msg.len());
    buf.put(msg);

    assert_eq!(None, codec.decode(buf).unwrap());
    buf.extend_from_slice(&b"OK\n"[..]);
    assert_eq!(Response::Empty, codec.decode(buf).unwrap().unwrap());
}

#[test]
fn decoder_invalid_greeting() {
    let mut codec = MpdCodec::new();
    let buf = &mut BytesMut::new();

    let msg = "ASDF\n";
    buf.reserve(msg.len());
    buf.put(msg);

    assert!(codec.decode(buf).is_err());
}

#[test]
fn decoder_incomplete_response() {
    let mut codec = MpdCodec::new_greeted();
    let buf = &mut BytesMut::new();

    let msg = "hello: world\nOK"; // Note missing final newline
    buf.reserve(msg.len());
    buf.put(msg);

    assert_eq!(None, codec.decode(buf).unwrap());
}

#[test]
fn decoder_empty_response() {
    let mut codec = MpdCodec::new_greeted();
    let buf = &mut BytesMut::new();

    let msg = "OK\nOK\n";
    buf.reserve(msg.len());
    buf.put(msg);

    assert_eq!(Response::Empty, codec.decode(buf).unwrap().unwrap());
    // Test if it leaves trailing data alone
    assert_eq!(buf.len(), 3);
    // Test if it parses consecutive messages
    assert_eq!(Response::Empty, codec.decode(buf).unwrap().unwrap());
}

#[test]
fn decoder_simple_response() {
    let mut codec = MpdCodec::new_greeted();
    let buf = &mut BytesMut::new();

    let msg = "key: value\nfoo:   bar\nbaz: qux    \nbaz: qux2\nOK\n";
    buf.reserve(msg.len());
    buf.put(msg);

    let mut map = HashMap::new();
    map.insert(String::from("key"), vec![String::from("value")]);
    // Values can contain leading or trailing spaces
    map.insert(String::from("foo"), vec![String::from("  bar")]);
    map.insert(
        String::from("baz"),
        vec![String::from("qux    "), String::from("qux2")],
    );

    assert_eq!(Response::Simple(map), codec.decode(buf).unwrap().unwrap());
}

#[test]
fn decoder_error_response() {
    let mut codec = MpdCodec::new_greeted();
    let buf = &mut BytesMut::new();

    // error message returned when trying to play a song not in the queue
    let msg = "ACK [2@0] {play} Bad song index\n";
    buf.reserve(msg.len());
    buf.put(msg);

    assert_eq!(
        Response::Error {
            error_code: 2,
            command_list_index: 0,
            current_command: String::from("play"),
            message: String::from("Bad song index"),
        },
        codec.decode(buf).unwrap().unwrap()
    );

    let msg = "ACK [5@0] {} unknown command \"asdf\"\n";
    buf.reserve(msg.len());
    buf.put(msg);

    assert_eq!(
        Response::Error {
            error_code: 5,
            command_list_index: 0,
            current_command: String::new(),
            message: String::from("unknown command \"asdf\""),
        },
        codec.decode(buf).unwrap().unwrap()
    );
}

#[test]
fn decoder_terminator_in_values() {
    let mut codec = MpdCodec::new_greeted();
    let buf = &mut BytesMut::new();

    let msg = "foo: OK\nOK\n";
    buf.reserve(msg.len());
    buf.put(msg);

    let mut map = HashMap::new();
    map.insert(String::from("foo"), vec![String::from("OK")]);

    assert_eq!(Response::Simple(map), codec.decode(buf).unwrap().unwrap());
}

#[test]
fn decoder_gracefully_handles_unicode_errors() {
    let mut codec = MpdCodec::new_greeted();
    // Invalid byte, newline, followed by "OK\n" terminator
    let buf = &mut BytesMut::from(&[0x80, 0x0a, 0x4f, 0x4b, 0x0a][..]);

    assert!(codec.decode(buf).is_err());
}
