use tokio_test::io::Builder as MockBuilder;
use futures::{stream::StreamExt, sink::SinkExt};

use mpd_protocol::{Command, MpdCodec};

#[tokio::test]
async fn full_interaction() {
    let io = MockBuilder::new()
        .read(b"OK MPD 0.21.11\n")
        .write(b"status\n")
        .read(b"foo: bar\nOK\n")
        .build();

    let mut conn = MpdCodec::connect(io).await.unwrap();
    assert_eq!(conn.codec().protocol_version(), "0.21.11");

    conn.send(Command::new("status")).await.unwrap();

    let response = conn.next().await.unwrap().unwrap();
    assert_eq!(response.len(), 1);

    let frame = response.single_frame().unwrap();
    assert_eq!(frame.find("foo"), Some("bar"));
}
