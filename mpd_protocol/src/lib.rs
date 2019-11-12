pub mod codec;
pub mod response;

pub use codec::Codec;
pub use response::Response;

#[cfg(test)]
mod tests {
    use bytes::{BufMut, BytesMut};
    use tokio::codec::Decoder;

    use std::collections::HashMap;

    use super::*;

    #[test]
    fn decoder_incomplete_response() {
        let mut codec = Codec::new();
        let buf = &mut BytesMut::new();

        let msg = "hello: world\nOK"; // Note missing final newline
        buf.reserve(msg.len());
        buf.put(msg);

        assert_eq!(None, codec.decode(buf).unwrap());
    }

    #[test]
    fn decoder_empty_response() {
        let mut codec = Codec::new();
        let buf = &mut BytesMut::new();

        let msg = "OK\nhello";
        buf.reserve(msg.len());
        buf.put(msg);

        assert_eq!(Response::Empty, codec.decode(buf).unwrap().unwrap(),);
        // Test if it leaves trailing data alone
        assert_eq!(buf.len(), 5);
    }

    #[test]
    fn decoder_simple_response() {
        let mut codec = Codec::new();
        let buf = &mut BytesMut::new();

        let msg = "key: value\nfoo:   bar\nbaz: qux    \nbaz: qux2\nOK\n";
        buf.reserve(msg.len());
        buf.put(msg);

        let mut map = HashMap::new();
        map.insert(String::from("key"), vec![String::from("value")]);
        map.insert(String::from("foo"), vec![String::from("bar")]);
        map.insert(
            String::from("baz"),
            vec![String::from("qux"), String::from("qux2")],
        );

        assert_eq!(Response::Simple(map), codec.decode(buf).unwrap().unwrap());
    }

    #[test]
    fn decoder_error_response() {
        let mut codec = Codec::new();
        let buf = &mut BytesMut::new();

        // error message returned when trying to play a song not in the queue
        let msg = "ACK [2@0] {play} Bad song index";
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
    }
}
