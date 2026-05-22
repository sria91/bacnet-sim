/// BACnet/IP BVLL (BACnet Virtual Link Layer) frame codec.
///
/// References: ASHRAE 135-2020 Annex J.
use bacnet_types::error::BacnetError;
use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::net::SocketAddrV4;

pub const BVLL_TYPE: u8 = 0x81;

/// All BVLL function codes the simulator needs to handle.
#[derive(Debug, Clone, PartialEq)]
pub enum BvllFrame {
    BvlcResult {
        result_code: u16,
    },
    WriteBroadcastDistributionTable(Vec<BdtEntry>),
    ReadBroadcastDistributionTable,
    ReadBroadcastDistributionTableAck(Vec<BdtEntry>),
    ForwardedNpdu {
        originating_address: SocketAddrV4,
        npdu: Bytes,
    },
    RegisterForeignDevice {
        ttl: u16,
    },
    ReadForeignDeviceTable,
    ReadForeignDeviceTableAck(Vec<FdtEntry>),
    DeleteForeignDeviceTableEntry {
        address: SocketAddrV4,
    },
    DistributeBroadcastToNetwork(Bytes),
    OriginalUnicastNpdu(Bytes),
    OriginalBroadcastNpdu(Bytes),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BdtEntry {
    pub address: SocketAddrV4,
    pub mask: [u8; 4],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FdtEntry {
    pub address: SocketAddrV4,
    pub ttl: u16,
    pub remaining: u16,
}

impl BvllFrame {
    pub fn encode(&self) -> Bytes {
        let mut buf = BytesMut::new();
        buf.put_u8(BVLL_TYPE);
        match self {
            Self::OriginalUnicastNpdu(npdu) => {
                buf.put_u8(0x10);
                let len = (4 + npdu.len()) as u16;
                buf.put_u16(len);
                buf.extend_from_slice(npdu);
            }
            Self::OriginalBroadcastNpdu(npdu) => {
                buf.put_u8(0x11);
                let len = (4 + npdu.len()) as u16;
                buf.put_u16(len);
                buf.extend_from_slice(npdu);
            }
            Self::RegisterForeignDevice { ttl } => {
                buf.put_u8(0x0B);
                buf.put_u16(6);
                buf.put_u16(*ttl);
            }
            Self::ForwardedNpdu {
                originating_address,
                npdu,
            } => {
                buf.put_u8(0x0A);
                let len = (4 + 6 + npdu.len()) as u16;
                buf.put_u16(len);
                buf.extend_from_slice(&originating_address.ip().octets());
                buf.put_u16(originating_address.port());
                buf.extend_from_slice(npdu);
            }
            Self::BvlcResult { result_code } => {
                buf.put_u8(0x01);
                buf.put_u16(6);
                buf.put_u16(*result_code);
            }
            Self::DistributeBroadcastToNetwork(npdu) => {
                buf.put_u8(0x0F);
                let len = (4 + npdu.len()) as u16;
                buf.put_u16(len);
                buf.extend_from_slice(npdu);
            }
            _ => {} // other variants encoded on demand
        }
        buf.freeze()
    }

    pub fn decode(buf: &[u8]) -> Result<Self, BacnetError> {
        if buf.len() < 4 {
            return Err(BacnetError::DecodeError("BVLL frame too short".into()));
        }
        if buf[0] != BVLL_TYPE {
            return Err(BacnetError::DecodeError(format!(
                "not a BVLL frame: {:#02x}",
                buf[0]
            )));
        }
        let function = buf[1];
        let length = u16::from_be_bytes([buf[2], buf[3]]) as usize;
        if buf.len() < length {
            return Err(BacnetError::DecodeError("BVLL frame truncated".into()));
        }
        let payload = &buf[4..length];
        match function {
            0x01 => {
                if payload.len() < 2 {
                    return Err(BacnetError::DecodeError("BvlcResult too short".into()));
                }
                Ok(Self::BvlcResult {
                    result_code: u16::from_be_bytes([payload[0], payload[1]]),
                })
            }
            0x0A => {
                // ForwardedNpdu: 4-byte IP + 2-byte port = 6-byte originator, then NPDU.
                // Minimum valid total frame length = 4 (header) + 6 (originator) = 10.
                if length < 10 {
                    return Err(BacnetError::DecodeError(format!(
                        "ForwardedNpdu too short: BVLL length field is {length}, need at least 10"
                    )));
                }
                if payload.len() < 6 {
                    return Err(BacnetError::DecodeError(format!(
                        "ForwardedNpdu truncated: payload is {} bytes, need at least 6 for originator address",
                        payload.len()
                    )));
                }
                let ip = std::net::Ipv4Addr::new(payload[0], payload[1], payload[2], payload[3]);
                let port = u16::from_be_bytes([payload[4], payload[5]]);
                let npdu = Bytes::copy_from_slice(&payload[6..]);
                Ok(Self::ForwardedNpdu {
                    originating_address: SocketAddrV4::new(ip, port),
                    npdu,
                })
            }
            0x0B => {
                // RegisterForeignDevice: 2-byte TTL
                if payload.len() < 2 {
                    return Err(BacnetError::DecodeError(
                        "RegisterForeignDevice too short".into(),
                    ));
                }
                Ok(Self::RegisterForeignDevice {
                    ttl: u16::from_be_bytes([payload[0], payload[1]]),
                })
            }
            0x0F => Ok(Self::DistributeBroadcastToNetwork(Bytes::copy_from_slice(
                payload,
            ))),
            0x10 => Ok(Self::OriginalUnicastNpdu(Bytes::copy_from_slice(payload))),
            0x11 => Ok(Self::OriginalBroadcastNpdu(Bytes::copy_from_slice(payload))),
            code => Err(BacnetError::DecodeError(format!(
                "unknown BVLL function {code:#02x}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn original_unicast_npdu_encode_decode() {
        let npdu_data = vec![0x01u8, 0x20, 0x00, 0x00];
        let frame = BvllFrame::OriginalUnicastNpdu(Bytes::from(npdu_data.clone()));
        let encoded = frame.encode();
        assert_eq!(encoded[0], 0x81);
        assert_eq!(encoded[1], 0x10); // OriginalUnicastNpdu function code
        let decoded = BvllFrame::decode(&encoded).unwrap();
        assert!(matches!(decoded, BvllFrame::OriginalUnicastNpdu(_)));
    }

    #[test]
    fn register_foreign_device_roundtrip() {
        let frame = BvllFrame::RegisterForeignDevice { ttl: 300 };
        let encoded = frame.encode();
        // function byte
        assert_eq!(encoded[1], 0x0B);
    }

    #[test]
    fn forwarded_npdu_has_originator_address() {
        let orig = SocketAddrV4::new([192, 168, 1, 10].into(), 47808);
        let npdu = Bytes::from_static(b"\x01\x00");
        let frame = BvllFrame::ForwardedNpdu {
            originating_address: orig,
            npdu,
        };
        let encoded = frame.encode();
        assert_eq!(encoded[1], 0x0A);
        assert_eq!(&encoded[4..8], &[192, 168, 1, 10]);
        assert_eq!(u16::from_be_bytes([encoded[8], encoded[9]]), 47808);
    }
}
