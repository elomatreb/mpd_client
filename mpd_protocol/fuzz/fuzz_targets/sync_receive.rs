#![no_main]
use libfuzzer_sys::fuzz_target;

use mpd_protocol::sync::receive;

use std::io::Cursor;

fuzz_target!(|data: &[u8]| {
    let mut io = Cursor::new(data);
    let _ = receive(&mut io);
});
