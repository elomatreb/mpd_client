use bytes::BytesMut;
use tokio_util::codec::Encoder;

use mpd_protocol::MpdCodec;

#[test]
fn encoder() {
    let mut codec = MpdCodec::new();
    let buf = &mut BytesMut::new();

    codec.encode(String::from("currentsong"), buf).unwrap();

    assert_eq!(&b"currentsong\n"[..], buf);
    buf.clear();

    assert!(codec.encode(String::new(), buf).is_err());
    assert!(codec.encode(String::from("hello\nworld"), buf).is_err());
    assert_eq!(0, buf.len());
}
