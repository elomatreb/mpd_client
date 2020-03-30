use bytes::{Bytes, BytesMut};
use tokio_util::codec::Decoder;

use std::sync::Arc;

use mpd_protocol::codec::MpdCodec;
use mpd_protocol::response::{Frame, Response};

fn init_buffer(msg: &[u8]) -> BytesMut {
    let mut buf = BytesMut::from("OK MPD 0.21.11\n");
    buf.extend_from_slice(msg);
    buf
}

#[test]
fn decoder_greeting() {
    let codec = &mut MpdCodec::new();
    let buf = &mut BytesMut::from("OK MPD 0.21.11"); // Note missing newline

    assert_eq!(None, codec.decode(buf).unwrap());
    assert_eq!(None, codec.protocol_version());

    buf.extend_from_slice(b"\n");

    assert_eq!(None, codec.decode(buf).unwrap());
    assert_eq!(Some("0.21.11"), codec.protocol_version());
}

#[test]
fn decoder_empty_response() {
    let codec = &mut MpdCodec::new();
    let buf = &mut init_buffer(b"OK");

    assert_eq!(None, codec.decode(buf).unwrap());

    buf.extend_from_slice(b"\n");

    assert_eq!(Some(Response::empty()), codec.decode(buf).unwrap());
}

#[test]
fn decoder_simple_response() {
    let codec = &mut MpdCodec::new();
    let buf = &mut init_buffer(b"hello: world\nfoo: OK\nbar: 1234\nOK");

    assert_eq!(None, codec.decode(buf).unwrap());

    buf.extend_from_slice(b"\n");

    assert_eq!(
        Some(Response::new(
            vec![Frame {
                values: vec![
                    (Arc::from("hello"), String::from("world")),
                    (Arc::from("foo"), String::from("OK")),
                    (Arc::from("bar"), String::from("1234")),
                ],
                binary: None,
            }],
            None,
        )),
        codec.decode(buf).unwrap()
    );
}

#[test]
fn decoder_command_list() {
    let codec = &mut MpdCodec::new();
    let buf = &mut init_buffer(b"list_OK\nfoo: bar\nlist_OK\nbinary: 6\nBINARY\nlist_OK\nOK");

    assert_eq!(None, codec.decode(buf).unwrap());

    buf.extend_from_slice(b"\n");

    assert_eq!(
        Some(Response::new(
            vec![
                Frame::empty(),
                Frame {
                    values: vec![(Arc::from("foo"), String::from("bar"))],
                    binary: None,
                },
                Frame {
                    values: Vec::new(),
                    binary: Some(Vec::from("BINARY")),
                }
            ],
            None,
        )),
        codec.decode(buf).unwrap()
    );
}

#[test]
fn decoder_binary_response() {
    let codec = &mut MpdCodec::new();
    let buf = &mut init_buffer(b"binary: 16\nHELLO \nOK\n");

    assert_eq!(None, codec.decode(buf).unwrap());

    buf.extend_from_slice(b" WORLD\nOK\n");

    assert_eq!(
        Some(Response::new(
            vec![Frame {
                values: Vec::new(),
                binary: Some(Vec::from("HELLO \nOK\n WORLD")),
            }],
            None,
        )),
        codec.decode(buf).unwrap()
    );
}

#[test]
fn decoder_multiple_messages() {
    let codec = &mut MpdCodec::new();
    let buf = &mut init_buffer(b"foo: bar\nOK\nhello: world\nOK\n");

    assert_eq!(
        Some(Response::new(
            vec![Frame {
                values: vec![(Arc::from("foo"), String::from("bar"))],
                binary: None,
            }],
            None,
        )),
        codec.decode(buf).unwrap()
    );
    assert_eq!(Bytes::from("hello: world\nOK\n"), &*buf);
    assert_eq!(
        Some(Response::new(
            vec![Frame {
                values: vec![(Arc::from("hello"), String::from("world"))],
                binary: None,
            }],
            None,
        )),
        codec.decode(buf).unwrap(),
    );
    assert!(buf.is_empty());
}

#[test]
fn decoder_cursor_reset() {
    let codec = &mut MpdCodec::new();
    let buf = &mut init_buffer(b"hello: world\nOK");

    assert_eq!(None, codec.decode(buf).unwrap());

    buf.extend_from_slice(b"\na: b\nOK\n");

    assert!(codec.decode(buf).unwrap().is_some());
    assert!(codec.decode(buf).unwrap().is_some());
}
