#![no_main]
use bacnet_types::encoding::asn1::{
    decode_application_bitstring, decode_application_date, decode_application_object_id,
    decode_application_real, decode_application_unsigned,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = decode_application_unsigned(data);
    let _ = decode_application_real(data);
    let _ = decode_application_object_id(data);
    let _ = decode_application_date(data);
    let _ = decode_application_bitstring(data);
});
