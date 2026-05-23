use crate::error::BacnetError;
use crate::object_types::ObjectType;
use crate::property_value::{BacnetDate, BitString, Weekday};
use crate::ObjectId;
/// BACnet ASN.1 application-tag encoding helpers.
///
/// Each `encode_*` function appends a fully-formed TLV to `buf`.
/// Each `decode_*` function parses from `buf` and returns `(value, bytes_consumed)`.
use bytes::BytesMut;

// ---------------------------------------------------------------------------
// Unsigned integer
// ---------------------------------------------------------------------------

pub fn encode_application_unsigned(buf: &mut BytesMut, value: u32) {
    let (len, bytes) = uint_bytes(value);
    buf.extend_from_slice(&[tag_byte(2, len)]);
    buf.extend_from_slice(&bytes[..len as usize]);
}

pub fn decode_application_unsigned(buf: &[u8]) -> Result<(u32, usize), BacnetError> {
    if buf.is_empty() {
        return Err(BacnetError::DecodeError("buffer empty".into()));
    }
    let tag = buf[0];
    if (tag >> 4) != 2 {
        return Err(BacnetError::DecodeError(format!(
            "expected unsigned tag, got {:#02x}",
            tag
        )));
    }
    let len = (tag & 0x07) as usize;
    if buf.len() < 1 + len {
        return Err(BacnetError::DecodeError(
            "buffer too short for unsigned".into(),
        ));
    }
    let mut v = 0u32;
    for &b in &buf[1..1 + len] {
        v = (v << 8) | b as u32;
    }
    Ok((v, 1 + len))
}

// ---------------------------------------------------------------------------
// Real (f32)
// ---------------------------------------------------------------------------

pub fn encode_application_real(buf: &mut BytesMut, value: f32) {
    buf.extend_from_slice(&[tag_byte(4, 4)]);
    buf.extend_from_slice(&value.to_be_bytes());
}

pub fn decode_application_real(buf: &[u8]) -> Result<(f32, usize), BacnetError> {
    if buf.len() < 5 {
        return Err(BacnetError::DecodeError("buffer too short for real".into()));
    }
    if (buf[0] >> 4) != 4 || (buf[0] & 0x07) != 4 {
        return Err(BacnetError::DecodeError(format!(
            "expected real tag, got {:#02x}",
            buf[0]
        )));
    }
    let v = f32::from_be_bytes([buf[1], buf[2], buf[3], buf[4]]);
    Ok((v, 5))
}

// ---------------------------------------------------------------------------
// Object identifier
// ---------------------------------------------------------------------------

pub fn encode_application_object_id(buf: &mut BytesMut, oid: ObjectId) {
    let type_code = oid.object_type as u32;
    let encoded = (type_code << 22) | (oid.instance & 0x3FFFFF);
    buf.extend_from_slice(&[0xC4]); // tag 12, len 4
    buf.extend_from_slice(&encoded.to_be_bytes());
}

pub fn decode_application_object_id(buf: &[u8]) -> Result<(ObjectId, usize), BacnetError> {
    if buf.len() < 5 {
        return Err(BacnetError::DecodeError(
            "buffer too short for object-id".into(),
        ));
    }
    if buf[0] != 0xC4 {
        return Err(BacnetError::DecodeError(format!(
            "expected object-id tag, got {:#02x}",
            buf[0]
        )));
    }
    let raw = u32::from_be_bytes([buf[1], buf[2], buf[3], buf[4]]);
    let type_code = (raw >> 22) as u16;
    let instance = raw & 0x3FFFFF;
    let object_type = ObjectType::from_u16(type_code)
        .ok_or_else(|| BacnetError::DecodeError(format!("unknown object type {type_code}")))?;
    Ok((
        ObjectId {
            object_type,
            instance,
        },
        5,
    ))
}

// ---------------------------------------------------------------------------
// Date
// ---------------------------------------------------------------------------

pub fn encode_application_date(buf: &mut BytesMut, date: BacnetDate) {
    buf.extend_from_slice(&[
        tag_byte(10, 4),
        (date.year.saturating_sub(1900)) as u8,
        date.month,
        date.day,
        date.weekday as u8,
    ]);
}

pub fn decode_application_date(buf: &[u8]) -> Result<(BacnetDate, usize), BacnetError> {
    if buf.len() < 5 {
        return Err(BacnetError::DecodeError("buffer too short for date".into()));
    }
    if (buf[0] >> 4) != 10 {
        return Err(BacnetError::DecodeError(format!(
            "expected date tag, got {:#02x}",
            buf[0]
        )));
    }
    let year = buf[1] as u16 + 1900;
    let month = buf[2];
    let day = buf[3];
    let weekday = match buf[4] {
        1 => Weekday::Monday,
        2 => Weekday::Tuesday,
        3 => Weekday::Wednesday,
        4 => Weekday::Thursday,
        5 => Weekday::Friday,
        6 => Weekday::Saturday,
        7 => Weekday::Sunday,
        _ => Weekday::Unspecified,
    };
    Ok((
        BacnetDate {
            year,
            month,
            day,
            weekday,
        },
        5,
    ))
}

// ---------------------------------------------------------------------------
// BitString
// ---------------------------------------------------------------------------

pub fn encode_application_bitstring(buf: &mut BytesMut, bits: &BitString) {
    let data = bits.bits();
    let unused = if data.is_empty() {
        0u8
    } else {
        (8 - data.len() % 8) as u8 % 8
    };
    let byte_count = data.len().div_ceil(8);
    let total_len = 1 + byte_count; // unused-bits octet + data bytes
                                    // BACnet LVT 0-4 = direct length; LVT 5 = "next byte is the actual length".
                                    // Bitstrings for ProtocolServicesSupported (40-bit) and
                                    // ProtocolObjectTypesSupported (32-bit) have total_len >= 5 and therefore
                                    // require the extended-length form.
    if total_len <= 4 {
        buf.extend_from_slice(&[tag_byte(8, total_len as u8), unused]);
    } else {
        buf.extend_from_slice(&[tag_byte(8, 5), total_len as u8, unused]);
    }
    let mut byte = 0u8;
    for (i, &bit) in data.iter().enumerate() {
        if bit {
            byte |= 0x80 >> (i % 8);
        }
        if i % 8 == 7 {
            buf.extend_from_slice(&[byte]);
            byte = 0;
        }
    }
    if !data.len().is_multiple_of(8) {
        buf.extend_from_slice(&[byte]);
    }
}

pub fn decode_application_bitstring(buf: &[u8]) -> Result<(BitString, usize), BacnetError> {
    if buf.len() < 2 {
        return Err(BacnetError::DecodeError(
            "buffer too short for bitstring".into(),
        ));
    }
    if (buf[0] >> 4) != 8 {
        return Err(BacnetError::DecodeError(format!(
            "expected bitstring tag, got {:#02x}",
            buf[0]
        )));
    }
    let lvt = (buf[0] & 0x07) as usize;
    // LVT 0-4 = direct length; LVT 5 = next 1 byte is actual length.
    let (total_len, value_start) = if lvt <= 4 {
        (lvt, 1usize)
    } else if lvt == 5 {
        if buf.len() < 3 {
            return Err(BacnetError::DecodeError(
                "buffer too short for extended-length bitstring".into(),
            ));
        }
        (buf[1] as usize, 2usize)
    } else {
        return Err(BacnetError::DecodeError(format!(
            "unsupported bitstring extended-length type LVT={lvt}"
        )));
    };
    if buf.len() < value_start + total_len {
        return Err(BacnetError::DecodeError(
            "buffer too short for bitstring data".into(),
        ));
    }
    let unused = buf[value_start] as usize;
    let data_bytes = &buf[value_start + 1..value_start + total_len];
    let bit_count = if data_bytes.is_empty() {
        0
    } else {
        data_bytes.len() * 8 - unused
    };
    let mut bits = Vec::with_capacity(bit_count);
    for (i, &byte) in data_bytes.iter().enumerate() {
        let limit = if i == data_bytes.len() - 1 {
            8 - unused
        } else {
            8
        };
        for j in 0..limit {
            bits.push((byte & (0x80 >> j)) != 0);
        }
    }
    Ok((BitString::from_bits(&bits), value_start + total_len))
}

// ---------------------------------------------------------------------------
// Context-tagged unsigned
// ---------------------------------------------------------------------------

pub fn encode_context_unsigned(buf: &mut BytesMut, context_tag: u8, value: u32) {
    let (len, bytes) = uint_bytes(value);
    buf.extend_from_slice(&[context_tag_byte(context_tag, len)]);
    buf.extend_from_slice(&bytes[..len as usize]);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build an application-tag byte: upper 4 bits = tag number, lower 3 bits = length.
/// (Only valid for length 0..=6; longer values require extended encoding.)
fn tag_byte(tag: u8, len: u8) -> u8 {
    (tag << 4) | (len & 0x07)
}

/// Build a context-tag byte (bit 3 set to 0 for primitive).
fn context_tag_byte(tag: u8, len: u8) -> u8 {
    ((tag & 0x0F) << 4) | (len & 0x07) | 0x08
}

/// Pack a u32 into the minimal number of big-endian bytes (1..=4).
fn uint_bytes(value: u32) -> (u8, [u8; 4]) {
    if value < 0x100 {
        (1, [value as u8, 0, 0, 0])
    } else if value < 0x10000 {
        let b = value.to_be_bytes();
        (2, [b[2], b[3], 0, 0])
    } else if value < 0x1000000 {
        let b = value.to_be_bytes();
        (3, [b[1], b[2], b[3], 0])
    } else {
        let b = value.to_be_bytes();
        (4, [b[0], b[1], b[2], b[3]])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object_types::ObjectType;

    #[test]
    fn encode_unsigned_one_byte() {
        let mut buf = BytesMut::new();
        encode_application_unsigned(&mut buf, 42);
        assert_eq!(buf.as_ref(), &[0x21, 0x2A]);
    }

    #[test]
    fn encode_unsigned_two_bytes() {
        let mut buf = BytesMut::new();
        encode_application_unsigned(&mut buf, 300);
        assert_eq!(buf.as_ref(), &[0x22, 0x01, 0x2C]);
    }

    #[test]
    fn encode_decode_real_roundtrip() {
        for &val in &[0.0f32, -1.0, f32::MAX, f32::MIN_POSITIVE, 3.14159] {
            let mut buf = BytesMut::new();
            encode_application_real(&mut buf, val);
            let (decoded, _) = decode_application_real(&buf).unwrap();
            assert!((decoded - val).abs() < f32::EPSILON || decoded == val);
        }
    }

    #[test]
    fn encode_object_id() {
        let oid = ObjectId {
            object_type: ObjectType::AnalogInput,
            instance: 7,
        };
        let mut buf = BytesMut::new();
        encode_application_object_id(&mut buf, oid);
        assert_eq!(buf.as_ref(), &[0xC4, 0x00, 0x00, 0x00, 0x07]);
    }

    #[test]
    fn decode_date() {
        let bytes = [0xA4, 0x79, 0x01, 0x01, 0x05]; // 2021-01-01 Friday
        let (date, _) = decode_application_date(&bytes).unwrap();
        assert_eq!(date.year, 2021);
        assert_eq!(date.month, 1);
        assert_eq!(date.day, 1);
        assert_eq!(date.weekday, Weekday::Friday);
    }

    #[test]
    fn decode_malformed_unsigned_tag_returns_error() {
        let bytes = [0xFF, 0xFF, 0xFF];
        assert!(decode_application_unsigned(&bytes).is_err());
    }

    #[test]
    fn context_tag_encoding_unsigned() {
        let mut buf = BytesMut::new();
        encode_context_unsigned(&mut buf, 3, 255);
        // context tag 3, length 1, value 0xFF → tag byte = (3<<4)|0x08|1 = 0x39
        assert_eq!(buf.as_ref(), &[0x39, 0xFF]);
    }

    #[test]
    fn bit_string_roundtrip() {
        let bits = BitString::from_bits(&[true, false, true, true, false]);
        let mut buf = BytesMut::new();
        encode_application_bitstring(&mut buf, &bits);
        let (decoded, _) = decode_application_bitstring(&buf).unwrap();
        assert_eq!(decoded.bits(), bits.bits());
    }
}
