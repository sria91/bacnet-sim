/// BACnet MS/TP frame codec.
///
/// References: ASHRAE 135-2020 Clause 9.
use bacnet_types::error::BacnetError;
use bytes::{BufMut, Bytes, BytesMut};

pub const MSTP_PREAMBLE_55: u8 = 0x55;
pub const MSTP_PREAMBLE_FF: u8 = 0xFF;

/// MS/TP standard frame types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MstpFrameType {
    Token = 0x00,
    PollForMaster = 0x01,
    ReplyToPollForMaster = 0x02,
    TestRequest = 0x03,
    TestResponse = 0x04,
    BacnetDataExpectingReply = 0x05,
    BacnetDataNotExpectingReply = 0x06,
    ReplyPostponed = 0x07,
}

impl MstpFrameType {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x00 => Some(Self::Token),
            0x01 => Some(Self::PollForMaster),
            0x02 => Some(Self::ReplyToPollForMaster),
            0x03 => Some(Self::TestRequest),
            0x04 => Some(Self::TestResponse),
            0x05 => Some(Self::BacnetDataExpectingReply),
            0x06 => Some(Self::BacnetDataNotExpectingReply),
            0x07 => Some(Self::ReplyPostponed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MstpDecodeError {
    BadPreamble,
    Incomplete,
    BadHeaderCrc,
    BadDataCrc,
    UnknownFrameType(u8),
}

impl From<MstpDecodeError> for BacnetError {
    fn from(e: MstpDecodeError) -> Self {
        BacnetError::DecodeError(format!("{e:?}"))
    }
}

/// A complete MS/TP frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MstpFrame {
    pub frame_type: MstpFrameType,
    pub destination: u8,
    pub source: u8,
    pub data: Bytes,
}

impl MstpFrame {
    /// Encode to wire bytes.
    pub fn encode(&self) -> Bytes {
        let mut buf = BytesMut::new();
        buf.put_u8(MSTP_PREAMBLE_55);
        buf.put_u8(MSTP_PREAMBLE_FF);
        buf.put_u8(self.frame_type as u8);
        buf.put_u8(self.destination);
        buf.put_u8(self.source);
        let data_len = self.data.len() as u16;
        buf.put_u16(data_len);
        let hdr_crc = crc8(&buf[2..7]);
        buf.put_u8(hdr_crc);
        if !self.data.is_empty() {
            buf.extend_from_slice(&self.data);
            let data_crc = crc16(&self.data);
            buf.put_u16_le(data_crc);
        }
        buf.freeze()
    }

    /// Decode from wire bytes.
    pub fn decode(buf: &[u8]) -> Result<Self, MstpDecodeError> {
        if buf.len() < 2 {
            return Err(MstpDecodeError::Incomplete);
        }
        if buf[0] != MSTP_PREAMBLE_55 || buf[1] != MSTP_PREAMBLE_FF {
            return Err(MstpDecodeError::BadPreamble);
        }
        if buf.len() < 8 {
            return Err(MstpDecodeError::Incomplete);
        }
        let frame_type =
            MstpFrameType::from_u8(buf[2]).ok_or(MstpDecodeError::UnknownFrameType(buf[2]))?;
        let destination = buf[3];
        let source = buf[4];
        let data_len = u16::from_be_bytes([buf[5], buf[6]]) as usize;
        let expected_hdr_crc = buf[7];
        let actual_hdr_crc = crc8(&buf[2..7]);
        if expected_hdr_crc != actual_hdr_crc {
            return Err(MstpDecodeError::BadHeaderCrc);
        }
        if buf.len() < 8 + data_len + if data_len > 0 { 2 } else { 0 } {
            return Err(MstpDecodeError::Incomplete);
        }
        let data = Bytes::copy_from_slice(&buf[8..8 + data_len]);
        if data_len > 0 {
            let expected_crc16 = u16::from_le_bytes([buf[8 + data_len], buf[9 + data_len]]);
            let actual_crc16 = crc16(&data);
            if expected_crc16 != actual_crc16 {
                return Err(MstpDecodeError::BadDataCrc);
            }
        }
        Ok(Self {
            frame_type,
            destination,
            source,
            data,
        })
    }
}

/// CRC-8 (IBM/ANSI) used for MS/TP header check.
pub fn crc8(data: &[u8]) -> u8 {
    let mut crc = 0xFFu8;
    for &b in data {
        crc ^= b;
        for _ in 0..8 {
            if crc & 0x01 != 0 {
                crc = (crc >> 1) ^ 0xE0;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

/// CRC-16 (IBM) used for MS/TP data check.
pub fn crc16(data: &[u8]) -> u16 {
    let mut crc = 0xFFFFu16;
    for &b in data {
        crc ^= b as u16;
        for _ in 0..8 {
            if crc & 0x0001 != 0 {
                crc = (crc >> 1) ^ 0xA001;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_frame_encode_header_crc() {
        let frame = MstpFrame {
            frame_type: MstpFrameType::Token,
            destination: 2,
            source: 1,
            data: Bytes::new(),
        };
        let encoded = frame.encode();
        assert_eq!(encoded[0], 0x55);
        assert_eq!(encoded[1], 0xFF);
        assert_eq!(encoded[2], 0x00);
        assert_eq!(encoded[3], 0x02);
        assert_eq!(encoded[4], 0x01);
        let expected_crc = crc8(&encoded[2..7]);
        assert_eq!(encoded[7], expected_crc);
    }

    #[test]
    fn data_frame_crc16_correct() {
        let data = Bytes::from_static(b"Hello BACnet");
        let frame = MstpFrame {
            frame_type: MstpFrameType::BacnetDataNotExpectingReply,
            destination: 5,
            source: 1,
            data: data.clone(),
        };
        let encoded = frame.encode();
        let crc16_pos = encoded.len() - 2;
        let expected = crc16(&encoded[8..crc16_pos]);
        let actual = u16::from_le_bytes([encoded[crc16_pos], encoded[crc16_pos + 1]]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn decode_incomplete_frame() {
        let partial = vec![0x55u8, 0xFF, 0x00];
        assert_eq!(
            MstpFrame::decode(&partial),
            Err(MstpDecodeError::Incomplete)
        );
    }

    #[test]
    fn decode_bad_preamble() {
        let bad = vec![0xAAu8, 0xBB, 0x00, 0x01, 0x02, 0x00, 0x00, 0x00];
        assert_eq!(MstpFrame::decode(&bad), Err(MstpDecodeError::BadPreamble));
    }

    #[test]
    fn roundtrip_data_frame() {
        let original = MstpFrame {
            frame_type: MstpFrameType::BacnetDataExpectingReply,
            destination: 3,
            source: 0,
            data: Bytes::from_static(b"\x01\x00\x00\xFF"),
        };
        let encoded = original.encode();
        let decoded = MstpFrame::decode(&encoded).unwrap();
        assert_eq!(decoded, original);
    }
}
