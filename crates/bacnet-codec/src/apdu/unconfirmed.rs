use bacnet_types::{
    encoding::{
        asn1::{encode_application_object_id, encode_application_unsigned},
        tags::{decode_ctx_u32, encode_ctx_u32, has_context_tag},
    },
    error::BacnetError,
    ObjectId, ObjectType,
};
use bytes::{BufMut, BytesMut};

use super::confirmed::WhoIsRequest;

/// I-Am response data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IAmRequest {
    pub device_instance: u32,
    pub max_apdu_length_accepted: u16,
    pub segmentation_supported: super::confirmed::Segmentation,
    pub vendor_id: u16,
}

/// Union of all unconfirmed service request bodies.
#[derive(Debug, Clone, PartialEq)]
pub enum UnconfirmedRequest {
    WhoIs(WhoIsRequest),
    IAm(IAmRequest),
    UnconfirmedCovNotification(UnconfirmedCovNotification),
    TimeSynchronization { datetime: bytes::Bytes },
}

/// Unconfirmed COV notification payload.
#[derive(Debug, Clone, PartialEq)]
pub struct UnconfirmedCovNotification {
    pub subscriber_process_id: u32,
    pub initiating_device: u32,
    pub monitored_object: bacnet_types::ObjectId,
    pub time_remaining: u32,
    pub list_of_values: Vec<(
        bacnet_types::PropertyIdentifier,
        bacnet_types::PropertyValue,
    )>,
}

impl UnconfirmedRequest {
    pub fn encode(&self, buf: &mut BytesMut) {
        match self {
            Self::WhoIs(w) => {
                buf.put_u8(0x10); // PDU type: unconfirmed
                buf.put_u8(0x08); // Who-Is service choice
                if let (Some(lo), Some(hi)) = (w.low_limit, w.high_limit) {
                    encode_ctx_u32(buf, 0, lo);
                    encode_ctx_u32(buf, 1, hi);
                }
            }
            Self::IAm(i) => {
                buf.put_u8(0x10);
                buf.put_u8(0x00); // I-Am service choice
                                  // I-Am uses application-tagged values (not context-tagged)
                let device_oid = ObjectId {
                    object_type: ObjectType::Device,
                    instance: i.device_instance,
                };
                encode_application_object_id(buf, device_oid);
                encode_application_unsigned(buf, i.max_apdu_length_accepted as u32);
                // Segmentation: enumerated (tag 9)
                let seg_val: u32 = match i.segmentation_supported {
                    super::confirmed::Segmentation::Both => 0,
                    super::confirmed::Segmentation::Transmit => 1,
                    super::confirmed::Segmentation::Receive => 2,
                    super::confirmed::Segmentation::NoSegmentation => 3,
                };
                // Enumerated application tag 9
                buf.put_u8(0x91); // tag 9, len 1
                buf.put_u8(seg_val as u8);
                // Vendor ID (Unsigned16)
                encode_application_unsigned(buf, i.vendor_id as u32);
            }
            _ => {}
        }
    }

    /// Decode an Unconfirmed-Request PDU from a raw APDU slice.
    ///
    /// Layout: `[PDU-type(1)] [service-choice(1)] [body...]`
    pub fn decode(buf: &[u8]) -> Result<Self, BacnetError> {
        if buf.len() < 2 {
            return Err(BacnetError::DecodeError("unconfirmed PDU too short".into()));
        }
        if buf[0] != 0x10 {
            return Err(BacnetError::DecodeError(format!(
                "expected unconfirmed PDU type, got {:#02x}",
                buf[0]
            )));
        }
        let body = &buf[2..];
        match buf[1] {
            // Who-Is (8): optional context [0] low, [1] high
            0x08 => {
                let mut pos = 0;
                let low_limit = if has_context_tag(body, pos, 0) {
                    Some(decode_ctx_u32(body, &mut pos, 0)?)
                } else {
                    None
                };
                let high_limit = if has_context_tag(body, pos, 1) {
                    Some(decode_ctx_u32(body, &mut pos, 1)?)
                } else {
                    None
                };
                Ok(Self::WhoIs(WhoIsRequest {
                    low_limit,
                    high_limit,
                }))
            }
            // I-Am (0): application-tagged device-id, max-apdu, segmentation, vendor-id
            0x00 => {
                // Minimal parse: just extract device-instance from first field
                let device_instance = if body.len() >= 5 && body[0] == 0xC4 {
                    // Application-tagged ObjectId: 0xC4 + 4 bytes
                    let raw = u32::from_be_bytes([body[1], body[2], body[3], body[4]]);
                    raw & 0x3F_FFFF
                } else {
                    0
                };
                Ok(Self::IAm(IAmRequest {
                    device_instance,
                    max_apdu_length_accepted: 1476,
                    segmentation_supported: super::confirmed::Segmentation::NoSegmentation,
                    vendor_id: 999,
                }))
            }
            code => Err(BacnetError::DecodeError(format!(
                "unknown unconfirmed service {code:#02x}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whois_encode_decode_roundtrip() {
        let req = UnconfirmedRequest::WhoIs(WhoIsRequest {
            low_limit: Some(1000),
            high_limit: Some(2000),
        });
        let mut buf = BytesMut::new();
        req.encode(&mut buf);
        let decoded = UnconfirmedRequest::decode(&buf).unwrap();
        match decoded {
            UnconfirmedRequest::WhoIs(w) => {
                assert_eq!(w.low_limit, Some(1000));
                assert_eq!(w.high_limit, Some(2000));
            }
            _ => panic!("expected WhoIs"),
        }
    }

    #[test]
    fn iam_encode_decode_roundtrip() {
        let iam = UnconfirmedRequest::IAm(IAmRequest {
            device_instance: 42,
            max_apdu_length_accepted: 1476,
            segmentation_supported: super::super::confirmed::Segmentation::NoSegmentation,
            vendor_id: 999,
        });
        let mut buf = BytesMut::new();
        iam.encode(&mut buf);
        let decoded = UnconfirmedRequest::decode(&buf).unwrap();
        match decoded {
            UnconfirmedRequest::IAm(i) => assert_eq!(i.device_instance, 42),
            _ => panic!("expected IAm"),
        }
    }
}
