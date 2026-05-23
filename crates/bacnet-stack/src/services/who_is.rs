use bacnet_codec::apdu::confirmed::Segmentation;
use bacnet_codec::apdu::unconfirmed::{IAmRequest, UnconfirmedRequest};
use bacnet_types::DeviceId;

/// Handle a Who-Is request: return `Some(I-Am)` if `device_id` is in range.
pub fn handle_who_is(
    low: Option<u32>,
    high: Option<u32>,
    device_id: DeviceId,
    max_apdu: u16,
    vendor_id: u16,
) -> Option<UnconfirmedRequest> {
    let id = device_id.0;
    let in_range = match (low, high) {
        (Some(lo), Some(hi)) => id >= lo && id <= hi,
        _ => true,
    };
    if in_range {
        Some(UnconfirmedRequest::IAm(IAmRequest {
            device_instance: id,
            max_apdu_length_accepted: max_apdu,
            segmentation_supported: Segmentation::NoSegmentation,
            vendor_id,
        }))
    } else {
        None
    }
}
