#![no_main]
use libfuzzer_sys::fuzz_target;
use bacnet_codec::sc::ScFrame;

fuzz_target!(|data: &[u8]| {
    let _ = ScFrame::decode(data);
    // If it decoded successfully, re-encoding must not panic.
    if let Ok(frame) = ScFrame::decode(data) {
        let _ = frame.encode();
    }
});
