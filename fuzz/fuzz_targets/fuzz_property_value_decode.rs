#![no_main]
use libfuzzer_sys::fuzz_target;
use bacnet_types::encoding::asn1::{
    decode_application_boolean, decode_application_real, decode_application_unsigned,
    decode_application_character_string, decode_application_enumerated,
    decode_application_object_id,
};

fuzz_target!(|data: &[u8]| {
    let _ = decode_application_unsigned(data);
    let _ = decode_application_real(data);
    let _ = decode_application_boolean(data);
    let _ = decode_application_character_string(data);
    let _ = decode_application_enumerated(data);
    let _ = decode_application_object_id(data);
});
