#![no_main]
use libfuzzer_sys::fuzz_target;
use bacnet_codec::npdu::Npdu;

fuzz_target!(|data: &[u8]| {
    let _ = Npdu::decode(data);
});
