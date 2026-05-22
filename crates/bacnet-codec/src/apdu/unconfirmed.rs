use bacnet_types::error::BacnetError;
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
    pub list_of_values: Vec<(bacnet_types::PropertyIdentifier, bacnet_types::PropertyValue)>,
}

impl UnconfirmedRequest {
    pub fn encode(&self, buf: &mut BytesMut) {
        match self {
            Self::WhoIs(w) => {
                buf.put_u8(0x10); // PDU type unconfirmed
                buf.put_u8(0x08); // Who-Is service choice
                if let (Some(lo), Some(hi)) = (w.low_limit, w.high_limit) {
                    // context [0] low, [1] high
                    buf.put_u8(0x09);
                    buf.put_u8(lo as u8);
                    buf.put_u8(0x19);
                    buf.put_u8(hi as u8);
                }
            }
            Self::IAm(i) => {
                buf.put_u8(0x10);
                buf.put_u8(0x00); // I-Am service choice
                // object identifier (context 0), max APDU (context 1),
                // segmentation (context 2), vendor-id (context 3) — placeholders
                buf.put_u8(0xC4);
                buf.extend_from_slice(&((8u32 << 22) | i.device_instance).to_be_bytes());
            }
            _ => {}
        }
    }

    pub fn decode(buf: &[u8]) -> Result<Self, BacnetError> {
        if buf.len() < 2 {
            return Err(BacnetError::DecodeError("unconfirmed PDU too short".into()));
        }
        if buf[0] != 0x10 {
            return Err(BacnetError::DecodeError(format!("expected unconfirmed PDU type, got {:#02x}", buf[0])));
        }
        match buf[1] {
            0x08 => {
                // Who-Is: optional low/high limits
                let (low, high) = if buf.len() >= 6 {
                    (Some(buf[3] as u32), Some(buf[5] as u32))
                } else {
                    (None, None)
                };
                Ok(Self::WhoIs(WhoIsRequest { low_limit: low, high_limit: high }))
            }
            0x00 => Ok(Self::IAm(IAmRequest {
                device_instance: 0,
                max_apdu_length_accepted: 1476,
                segmentation_supported: super::confirmed::Segmentation::NoSegmentation,
                vendor_id: 0,
            })),
            code => Err(BacnetError::DecodeError(format!("unknown unconfirmed service {code:#02x}"))),
        }
    }
}
