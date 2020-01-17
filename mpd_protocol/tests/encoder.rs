use bytes::BytesMut;
use tokio_util::codec::Encoder;

use mpd_protocol::Command;
use mpd_protocol::MpdCodec;

#[test]
fn encoder() {
    let mut codec = MpdCodec::new();
    let buf = &mut BytesMut::new();

    let command = Command::new("status");

    codec.encode(command.clone(), buf).unwrap();

    assert_eq!(command.render().as_bytes(), buf);
}
