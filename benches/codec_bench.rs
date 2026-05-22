use criterion::{criterion_group, criterion_main, Criterion, Throughput};

fn bench_bvll_encode(c: &mut Criterion) {
    use bacnet_codec::bvll::BvllFrame;
    use bytes::Bytes;

    let npdu = Bytes::from(vec![0x01u8, 0x00, 0x00, 0xFF]);
    let frame = BvllFrame::OriginalUnicastNpdu(npdu);

    let mut group = c.benchmark_group("bvll");
    group.throughput(Throughput::Elements(1));
    group.bench_function("encode_unicast", |b| {
        b.iter(|| {
            let _ = frame.encode();
        });
    });
    group.finish();
}

criterion_group!(benches, bench_bvll_encode);
criterion_main!(benches);
