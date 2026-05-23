/// Criterion benchmarks for the bacnet-codec crate.
///
/// Run with: cargo bench -p bacnet-codec
use bytes::{Bytes, BytesMut};
use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};

use bacnet_codec::{
    apdu::confirmed::{
        ConfirmedRequest, ConfirmedServiceRequest, MaxSegments, ReadPropertyRequest,
    },
    bvll::BvllFrame,
    mstp::{MstpFrame, MstpFrameType},
    npdu::Npdu,
    sc::ScFrame,
};
use bacnet_types::{property_id::PropertyIdentifier, ObjectId, ObjectType};

// ---------------------------------------------------------------------------
// BVLL
// ---------------------------------------------------------------------------

fn bench_bvll_encode(c: &mut Criterion) {
    let npdu = Bytes::from_static(b"\x01\x00\xFF\xFF\x00\x00\x30\x0C\x01\x0C");
    let frame = BvllFrame::OriginalUnicastNpdu(npdu);

    let mut group = c.benchmark_group("bvll");
    group.throughput(Throughput::Elements(1));

    group.bench_function("encode_unicast_npdu", |b| {
        b.iter(|| {
            let _ = black_box(frame.encode());
        });
    });

    let encoded = frame.encode();
    group.bench_function("decode_unicast_npdu", |b| {
        b.iter(|| {
            let _ = black_box(BvllFrame::decode(encoded.as_ref()));
        });
    });

    group.bench_function("roundtrip_unicast_npdu", |b| {
        b.iter(|| {
            let enc = frame.encode();
            let _ = black_box(BvllFrame::decode(enc.as_ref()));
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// NPDU
// ---------------------------------------------------------------------------

fn bench_npdu_encode_decode(c: &mut Criterion) {
    let apdu = b"\x00\x04\x00\x0C\x0C\x00\x00\x00\x01\x19\x55";

    let mut group = c.benchmark_group("npdu");
    group.throughput(Throughput::Elements(1));

    group.bench_function("encode_local", |b| {
        b.iter(|| {
            let _ = black_box(Npdu::encode_local(false, apdu));
        });
    });

    let encoded = Npdu::encode_local(false, apdu);
    group.bench_function("decode", |b| {
        b.iter(|| {
            let _ = black_box(Npdu::decode(encoded.as_ref()));
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// APDU — Confirmed ReadProperty
// ---------------------------------------------------------------------------

fn make_read_property_request() -> ConfirmedRequest {
    ConfirmedRequest {
        segmented_accepted: true,
        more_follows: false,
        segmented_response_accepted: true,
        max_segments: MaxSegments::Unspecified,
        max_response: 1476,
        invoke_id: 42,
        sequence_number: None,
        proposed_window: None,
        service: ConfirmedServiceRequest::ReadProperty(ReadPropertyRequest {
            object_id: ObjectId {
                object_type: ObjectType::AnalogInput,
                instance: 7,
            },
            property_id: PropertyIdentifier::PresentValue,
            array_index: None,
        }),
    }
}

fn bench_apdu_confirmed_encode_decode(c: &mut Criterion) {
    let req = make_read_property_request();

    let mut group = c.benchmark_group("apdu_confirmed");
    group.throughput(Throughput::Elements(1));

    group.bench_function("encode_read_property_req", |b| {
        b.iter(|| {
            let mut buf = BytesMut::new();
            black_box(&req).encode(&mut buf);
        });
    });

    let mut buf = BytesMut::new();
    req.encode(&mut buf);
    let bytes = buf.freeze();

    group.bench_function("decode_read_property_req", |b| {
        b.iter(|| {
            let _ = black_box(ConfirmedRequest::decode(bytes.as_ref()));
        });
    });

    group.bench_function("roundtrip_read_property_req", |b| {
        b.iter(|| {
            let mut buf = BytesMut::new();
            make_read_property_request().encode(&mut buf);
            let _ = black_box(ConfirmedRequest::decode(buf.as_ref()));
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// MS/TP
// ---------------------------------------------------------------------------

fn bench_mstp_encode_decode(c: &mut Criterion) {
    let data_frame = MstpFrame {
        frame_type: MstpFrameType::BacnetDataNotExpectingReply,
        destination: 5,
        source: 1,
        data: Bytes::from_static(
            b"\x01\x00\xFF\xFF\x00\x00\x30\x0C\x01\x0C\x0C\x00\x00\x00\x01\x19\x55",
        ),
    };
    let token_frame = MstpFrame {
        frame_type: MstpFrameType::Token,
        destination: 2,
        source: 1,
        data: Bytes::new(),
    };

    let mut group = c.benchmark_group("mstp");
    group.throughput(Throughput::Elements(1));

    group.bench_function("encode_data_frame", |b| {
        b.iter(|| {
            let _ = black_box(data_frame.encode());
        });
    });

    group.bench_function("encode_token_frame", |b| {
        b.iter(|| {
            let _ = black_box(token_frame.encode());
        });
    });

    let encoded_data = data_frame.encode();
    group.bench_function("decode_data_frame", |b| {
        b.iter(|| {
            let _ = black_box(MstpFrame::decode(encoded_data.as_ref()));
        });
    });

    let encoded_token = token_frame.encode();
    group.bench_function("decode_token_frame", |b| {
        b.iter(|| {
            let _ = black_box(MstpFrame::decode(encoded_token.as_ref()));
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// BACnet/SC
// ---------------------------------------------------------------------------

fn bench_sc_encode_decode(c: &mut Criterion) {
    let npdu = Bytes::from_static(b"\x01\x00\xFF\xFF\x00\x00\x30\x0C");
    let orig = [0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x01];
    let dest = [0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x02];
    let frame = ScFrame::encapsulated_npdu(1234, Some(orig), Some(dest), npdu);

    let mut group = c.benchmark_group("sc");
    group.throughput(Throughput::Elements(1));

    group.bench_function("encode_encapsulated_npdu", |b| {
        b.iter(|| {
            let _ = black_box(frame.encode());
        });
    });

    let encoded = frame.encode();
    group.bench_function("decode_encapsulated_npdu", |b| {
        b.iter(|| {
            let _ = black_box(ScFrame::decode(encoded.as_ref()));
        });
    });

    group.bench_function("roundtrip_encapsulated_npdu", |b| {
        b.iter(|| {
            let enc = frame.encode();
            let _ = black_box(ScFrame::decode(enc.as_ref()));
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Criterion entry points
// ---------------------------------------------------------------------------

criterion_group!(
    benches,
    bench_bvll_encode,
    bench_npdu_encode_decode,
    bench_apdu_confirmed_encode_decode,
    bench_mstp_encode_decode,
    bench_sc_encode_decode,
);
criterion_main!(benches);
