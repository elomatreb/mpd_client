use bytes::BytesMut;
use tokio_util::codec::Encoder;

use mpd_protocol::MpdCodec;
use mpd_protocol::{Command, CommandList};

#[test]
fn encoder() {
    let mut codec = MpdCodec::new();
    let buf = &mut BytesMut::new();

    let command = CommandList::new(Command::build("status").unwrap());

    codec.encode(command.clone(), buf).unwrap();

    assert_eq!(&b"status\n"[..], buf);
}
