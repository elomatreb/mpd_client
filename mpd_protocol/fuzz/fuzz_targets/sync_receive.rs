#![no_main]
use libfuzzer_sys::fuzz_target;

use mpd_protocol::Connection;

fuzz_target!(|data: &[u8]| {
    let mut connection = Connection::new_internal(data);
    let _ = connection.receive();
});
