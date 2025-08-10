use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use mpd_protocol::Connection;

// NOTE: Benchmark requires `--cfg criterion` to be set to build correctly.

const LONG_RESPONSE: &[u8] = include_bytes!("long.response");

pub fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("long response", |b| {
        b.iter(|| {
            let mut connection = Connection::new_internal(black_box(LONG_RESPONSE));
            let _ = connection.receive().unwrap();
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
