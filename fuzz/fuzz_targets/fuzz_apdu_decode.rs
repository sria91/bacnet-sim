#![no_main]
use bacnet_codec::apdu::{
    ack::ComplexAck, confirmed::ConfirmedRequest, unconfirmed::UnconfirmedRequest,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = ConfirmedRequest::decode(data);
    let _ = UnconfirmedRequest::decode(data);
    let _ = ComplexAck::decode(data);
});
