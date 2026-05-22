#![no_main]
use bacnet_codec::bvll::BvllFrame;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Must not panic on any input, however malformed.
    let _ = BvllFrame::decode(data);
});
