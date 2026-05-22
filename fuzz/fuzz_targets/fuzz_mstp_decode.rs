#![no_main]
use libfuzzer_sys::fuzz_target;
use bacnet_codec::mstp::MstpFrame;

fuzz_target!(|data: &[u8]| {
    let _ = MstpFrame::decode(data);
});
