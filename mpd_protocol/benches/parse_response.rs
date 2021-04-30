use criterion::{black_box, criterion_group, criterion_main, Criterion};
use mpd_protocol::sync::receive;

const LONG_RESPONSE: &[u8] = include_bytes!("long.response");

pub fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("long response", |b| {
        b.iter(|| {
            let _ = receive(black_box(LONG_RESPONSE));
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
