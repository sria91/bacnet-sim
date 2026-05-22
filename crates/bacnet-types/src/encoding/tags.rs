/// BACnet context-tag and application-tag decode helpers.
///
/// BACnet uses a compact TLV (tag, length, value) encoding.  The tag byte is:
///   bits 7-4  tag number (0-14; 15 = extended tag, next byte is actual number)
///   bit  3    class: 0 = application, 1 = context
///   bits 2-0  LVT:
///               0-4  direct length (number of value bytes)
///               5    extended length (next byte is the length)
///               6    opening tag (constructed encoding start, context only)
///               7    closing tag (constructed encoding end, context only)

use bytes::{BufMut, BytesMut};

use crate::{error::BacnetError, ObjectId, ObjectType, PropertyIdentifier, PropertyValue};
use crate::property_value::BitString;

// ---------------------------------------------------------------------------
// Primitive helpers
// ---------------------------------------------------------------------------

/// Peek at `buf[pos]` without advancing.
#[inline]
fn peek(buf: &[u8], pos: usize) -> Option<u8> {
    buf.get(pos).copied()
}

/// Decode the LVT-encoded length.  For opening/closing tags (lvt 6/7) this
/// should NOT be called — the caller handles those separately.
fn decode_lvt_len(buf: &[u8], pos: &mut usize, lvt: u8) -> Result<usize, BacnetError> {
    if lvt < 5 {
        Ok(lvt as usize)
    } else if lvt == 5 {
        let l = buf
            .get(*pos)
            .copied()
            .ok_or_else(|| BacnetError::DecodeError("extended-length byte missing".into()))?
            as usize;
        *pos += 1;
        Ok(l)
    } else {
        Err(BacnetError::DecodeError(format!("unexpected LVT {lvt} for length field")))
    }
}

/// Read `len` bytes from `buf[*pos..]` as a big-endian unsigned integer.
fn read_uint(buf: &[u8], pos: &mut usize, len: usize) -> Result<u32, BacnetError> {
    if *pos + len > buf.len() {
        return Err(BacnetError::DecodeError("integer value truncated".into()));
    }
    let mut v = 0u32;
    for &b in &buf[*pos..*pos + len] {
        v = (v << 8) | b as u32;
    }
    *pos += len;
    Ok(v)
}

// ---------------------------------------------------------------------------
// Context tag presence checks
// ---------------------------------------------------------------------------

/// Returns `true` if `buf[pos]` is an opening context tag with `number`.
pub fn is_opening(buf: &[u8], pos: usize, number: u8) -> bool {
    peek(buf, pos)
        .map(|b| (b & 0x08) != 0 && ((b >> 4) & 0x0F) == number && (b & 0x07) == 6)
        .unwrap_or(false)
}

/// Returns `true` if `buf[pos]` is a closing context tag with `number`.
pub fn is_closing(buf: &[u8], pos: usize, number: u8) -> bool {
    peek(buf, pos)
        .map(|b| (b & 0x08) != 0 && ((b >> 4) & 0x0F) == number && (b & 0x07) == 7)
        .unwrap_or(false)
}

/// Returns `true` if `buf[pos]` is a context-tagged non-opening/closing value
/// with tag `number` (i.e. class=context, lvt 0-5).
pub fn has_context_tag(buf: &[u8], pos: usize, number: u8) -> bool {
    peek(buf, pos)
        .map(|b| {
            let is_ctx = (b & 0x08) != 0;
            let tag = (b >> 4) & 0x0F;
            let lvt = b & 0x07;
            is_ctx && tag == number && lvt < 6
        })
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Context-tagged decode functions (advance `pos` on success)
// ---------------------------------------------------------------------------

/// Decode a context-tagged `u32` at tag `expected`.
pub fn decode_ctx_u32(
    buf: &[u8],
    pos: &mut usize,
    expected: u8,
) -> Result<u32, BacnetError> {
    let b = peek(buf, *pos)
        .ok_or_else(|| BacnetError::DecodeError("buffer exhausted (ctx u32)".into()))?;
    let tag = (b >> 4) & 0x0F;
    let is_ctx = (b & 0x08) != 0;
    let lvt = b & 0x07;
    if !is_ctx || tag != expected {
        return Err(BacnetError::DecodeError(format!(
            "expected context tag {expected}, got byte {b:#04x}"
        )));
    }
    *pos += 1;
    let len = decode_lvt_len(buf, pos, lvt)?;
    read_uint(buf, pos, len)
}

/// Decode a context-tagged `bool` at tag `expected`.
pub fn decode_ctx_bool(
    buf: &[u8],
    pos: &mut usize,
    expected: u8,
) -> Result<bool, BacnetError> {
    let v = decode_ctx_u32(buf, pos, expected)?;
    Ok(v != 0)
}

/// Decode a context-tagged `ObjectId` (always 4 value bytes) at tag `expected`.
pub fn decode_ctx_object_id(
    buf: &[u8],
    pos: &mut usize,
    expected: u8,
) -> Result<ObjectId, BacnetError> {
    let b = peek(buf, *pos)
        .ok_or_else(|| BacnetError::DecodeError("buffer exhausted (ctx object-id)".into()))?;
    let tag = (b >> 4) & 0x0F;
    let is_ctx = (b & 0x08) != 0;
    let lvt = b & 0x07;
    if !is_ctx || tag != expected {
        return Err(BacnetError::DecodeError(format!(
            "expected context tag {expected} for object-id, got {b:#04x}"
        )));
    }
    *pos += 1;
    let len = decode_lvt_len(buf, pos, lvt)?;
    if len != 4 {
        return Err(BacnetError::DecodeError(format!(
            "object-id must be 4 bytes, got {len}"
        )));
    }
    if *pos + 4 > buf.len() {
        return Err(BacnetError::DecodeError("object-id value truncated".into()));
    }
    let raw = u32::from_be_bytes([buf[*pos], buf[*pos + 1], buf[*pos + 2], buf[*pos + 3]]);
    *pos += 4;
    let type_code = (raw >> 22) as u16;
    let instance = raw & 0x3F_FFFF;
    Ok(ObjectId {
        object_type: ObjectType::from_u16(type_code).unwrap_or(ObjectType::Device),
        instance,
    })
}

/// Decode a context-tagged `PropertyIdentifier` at tag `expected`.
pub fn decode_ctx_property_id(
    buf: &[u8],
    pos: &mut usize,
    expected: u8,
) -> Result<PropertyIdentifier, BacnetError> {
    let v = decode_ctx_u32(buf, pos, expected)?;
    Ok(PropertyIdentifier::from_u32(v))
}

// ---------------------------------------------------------------------------
// Application-tagged decode
// ---------------------------------------------------------------------------

/// Decode one application-tagged `PropertyValue` from `buf[*pos..]`.
/// Advances `*pos` by the number of bytes consumed.
pub fn decode_application_value(
    buf: &[u8],
    pos: &mut usize,
) -> Result<PropertyValue, BacnetError> {
    let b = peek(buf, *pos)
        .ok_or_else(|| BacnetError::DecodeError("buffer exhausted (application value)".into()))?;
    let tag = (b >> 4) & 0x0F;
    let is_ctx = (b & 0x08) != 0;
    let lvt = b & 0x07;

    if is_ctx {
        return Err(BacnetError::DecodeError(format!(
            "expected application tag, got context byte {b:#04x}"
        )));
    }

    *pos += 1; // consume tag byte

    match tag {
        // Null (tag 0) — no value bytes; LVT is always 0
        0 => Ok(PropertyValue::Null),

        // Boolean (tag 1) — value encoded in LVT itself
        1 => Ok(PropertyValue::Boolean(lvt != 0)),

        // Unsigned integer (tag 2)
        2 => {
            let len = decode_lvt_len(buf, pos, lvt)?;
            let v = read_uint(buf, pos, len)?;
            Ok(PropertyValue::Unsigned(v))
        }

        // Signed integer (tag 3)
        3 => {
            let len = decode_lvt_len(buf, pos, lvt)?;
            if *pos + len > buf.len() {
                return Err(BacnetError::DecodeError("integer truncated".into()));
            }
            let mut v = 0i32;
            for &byte in &buf[*pos..*pos + len] {
                v = (v << 8) | byte as i32;
            }
            *pos += len;
            Ok(PropertyValue::Integer(v))
        }

        // Real (tag 4) — always 4 bytes
        4 => {
            let _len = decode_lvt_len(buf, pos, lvt)?;
            if *pos + 4 > buf.len() {
                return Err(BacnetError::DecodeError("real truncated".into()));
            }
            let v =
                f32::from_be_bytes([buf[*pos], buf[*pos + 1], buf[*pos + 2], buf[*pos + 3]]);
            *pos += 4;
            Ok(PropertyValue::Real(v))
        }

        // Double (tag 5) — always 8 bytes
        5 => {
            let _len = decode_lvt_len(buf, pos, lvt)?;
            if *pos + 8 > buf.len() {
                return Err(BacnetError::DecodeError("double truncated".into()));
            }
            let bytes: [u8; 8] = buf[*pos..*pos + 8].try_into().unwrap();
            *pos += 8;
            Ok(PropertyValue::Double(f64::from_be_bytes(bytes)))
        }

        // OctetString (tag 6)
        6 => {
            let len = decode_lvt_len(buf, pos, lvt)?;
            if *pos + len > buf.len() {
                return Err(BacnetError::DecodeError("octet-string truncated".into()));
            }
            let data = bytes::Bytes::copy_from_slice(&buf[*pos..*pos + len]);
            *pos += len;
            Ok(PropertyValue::OctetString(data))
        }

        // CharacterString (tag 7) — first byte is character-set (0 = UTF-8)
        7 => {
            let len = decode_lvt_len(buf, pos, lvt)?;
            if len == 0 || *pos + len > buf.len() {
                return Err(BacnetError::DecodeError("character-string invalid".into()));
            }
            let _charset = buf[*pos]; // 0x00 = UTF-8
            let s = String::from_utf8_lossy(&buf[*pos + 1..*pos + len]).into_owned();
            *pos += len;
            Ok(PropertyValue::CharacterString(s))
        }

        // BitString (tag 8) — first byte is unused-bits count
        8 => {
            let len = decode_lvt_len(buf, pos, lvt)?;
            if len == 0 || *pos + len > buf.len() {
                return Err(BacnetError::DecodeError("bit-string invalid".into()));
            }
            let unused = buf[*pos] as usize;
            let data = &buf[*pos + 1..*pos + len];
            let total = data.len() * 8 - unused;
            let mut bits = Vec::with_capacity(total);
            for (i, &byte) in data.iter().enumerate() {
                let nbits = if i + 1 == data.len() { 8 - unused } else { 8 };
                for bit in (0..nbits).rev() {
                    bits.push((byte >> bit) & 1 != 0);
                }
            }
            *pos += len;
            Ok(PropertyValue::BitString(BitString::from_bits(&bits)))
        }

        // Enumerated (tag 9)
        9 => {
            let len = decode_lvt_len(buf, pos, lvt)?;
            let v = read_uint(buf, pos, len)?;
            Ok(PropertyValue::Enumerated(v))
        }

        // Date (tag 10) — always 4 bytes: year-1900, month, day, weekday
        10 => {
            let _len = decode_lvt_len(buf, pos, lvt)?;
            if *pos + 4 > buf.len() {
                return Err(BacnetError::DecodeError("date truncated".into()));
            }
            use crate::property_value::{BacnetDate, Weekday};
            let year = buf[*pos] as u16 + 1900;
            let month = buf[*pos + 1];
            let day = buf[*pos + 2];
            let wd_raw = buf[*pos + 3];
            let weekday = match wd_raw {
                1 => Weekday::Monday,
                2 => Weekday::Tuesday,
                3 => Weekday::Wednesday,
                4 => Weekday::Thursday,
                5 => Weekday::Friday,
                6 => Weekday::Saturday,
                _ => Weekday::Sunday,
            };
            *pos += 4;
            Ok(PropertyValue::Date(BacnetDate { year, month, day, weekday }))
        }

        // Time (tag 11) — always 4 bytes: hour, minute, second, hundredths
        11 => {
            let _len = decode_lvt_len(buf, pos, lvt)?;
            if *pos + 4 > buf.len() {
                return Err(BacnetError::DecodeError("time truncated".into()));
            }
            use crate::property_value::BacnetTime;
            let t = BacnetTime {
                hour:        buf[*pos],
                minute:      buf[*pos + 1],
                second:      buf[*pos + 2],
                hundredths:  buf[*pos + 3],
            };
            *pos += 4;
            Ok(PropertyValue::Time(t))
        }

        // ObjectIdentifier (tag 12) — always 4 bytes
        12 => {
            let _len = decode_lvt_len(buf, pos, lvt)?;
            if *pos + 4 > buf.len() {
                return Err(BacnetError::DecodeError("object-id truncated".into()));
            }
            let raw = u32::from_be_bytes([
                buf[*pos],
                buf[*pos + 1],
                buf[*pos + 2],
                buf[*pos + 3],
            ]);
            *pos += 4;
            let type_code = (raw >> 22) as u16;
            let instance = raw & 0x3F_FFFF;
            Ok(PropertyValue::ObjectId(ObjectId {
                object_type: ObjectType::from_u16(type_code)
                    .unwrap_or(ObjectType::Device),
                instance,
            }))
        }

        other => Err(BacnetError::DecodeError(format!(
            "unsupported application tag {other:#x}"
        ))),
    }
}

// ---------------------------------------------------------------------------
// Context-tagged encode helpers (used by ComplexAck encoders)
// ---------------------------------------------------------------------------

/// Encode a context-tagged ObjectIdentifier (`[tag_num]` 4 bytes).
pub fn encode_ctx_object_id(buf: &mut BytesMut, tag_num: u8, oid: ObjectId) {
    let type_code = oid.object_type as u32;
    let raw = (type_code << 22) | (oid.instance & 0x3F_FFFF);
    // Context tag byte: (tag_num << 4) | 0x08 | 4
    buf.put_u8((tag_num << 4) | 0x08 | 4);
    buf.put_u32(raw);
}

/// Encode a context-tagged unsigned integer.
pub fn encode_ctx_u32(buf: &mut BytesMut, tag_num: u8, value: u32) {
    let (len, bytes) = uint_bytes(value);
    buf.put_u8((tag_num << 4) | 0x08 | len);
    buf.extend_from_slice(&bytes[..len as usize]);
}

/// Encode an opening context tag byte.
#[inline]
pub fn encode_opening_tag(buf: &mut BytesMut, tag_num: u8) {
    buf.put_u8((tag_num << 4) | 0x08 | 6);
}

/// Encode a closing context tag byte.
#[inline]
pub fn encode_closing_tag(buf: &mut BytesMut, tag_num: u8) {
    buf.put_u8((tag_num << 4) | 0x08 | 7);
}

// ---------------------------------------------------------------------------
// Internal byte-sizing helper (shared with asn1.rs)
// ---------------------------------------------------------------------------

fn uint_bytes(value: u32) -> (u8, [u8; 4]) {
    if value < 0x100 {
        (1, [value as u8, 0, 0, 0])
    } else if value < 0x10000 {
        (2, [(value >> 8) as u8, value as u8, 0, 0])
    } else if value < 0x1000000 {
        (3, [(value >> 16) as u8, (value >> 8) as u8, value as u8, 0])
    } else {
        (4, [
            (value >> 24) as u8,
            (value >> 16) as u8,
            (value >> 8) as u8,
            value as u8,
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_ctx_u32_single_byte() {
        // Context tag 0, len 1, value 85 (PresentValue)
        let buf = [0x09, 0x55u8]; // (0<<4)|0x08|1 = 0x09
        let mut pos = 0;
        let v = decode_ctx_u32(&buf, &mut pos, 0).unwrap();
        assert_eq!(v, 0x55);
        assert_eq!(pos, 2);
    }

    #[test]
    fn decode_ctx_object_id_basic() {
        // Context tag 0, len 4: AnalogInput instance 3
        // ObjectType::AnalogInput = 0, so raw = (0 << 22) | 3 = 3
        let raw: u32 = 3;
        let b0 = 0x0C_u8; // (0<<4)|0x08|4
        let mut buf = vec![b0];
        buf.extend_from_slice(&raw.to_be_bytes());
        let mut pos = 0;
        let oid = decode_ctx_object_id(&buf, &mut pos, 0).unwrap();
        assert_eq!(oid.instance, 3);
        assert_eq!(oid.object_type, crate::ObjectType::AnalogInput);
    }

    #[test]
    fn is_opening_closing() {
        let buf = [0x3E, 0x3F]; // opening tag 3, closing tag 3
        assert!(is_opening(&buf, 0, 3));
        assert!(!is_opening(&buf, 0, 4));
        assert!(is_closing(&buf, 1, 3));
        assert!(!is_closing(&buf, 1, 2));
    }

    #[test]
    fn decode_application_real() {
        let v: f32 = 22.5;
        let mut buf = BytesMut::new();
        buf.put_u8(0x44); // tag 4, len 4
        buf.extend_from_slice(&v.to_be_bytes());
        let mut pos = 0;
        let pv = decode_application_value(&buf, &mut pos).unwrap();
        assert_eq!(pv, PropertyValue::Real(22.5));
    }

    #[test]
    fn encode_decode_ctx_object_id() {
        let oid = ObjectId {
            object_type: crate::ObjectType::Device,
            instance: 1234,
        };
        let mut buf = BytesMut::new();
        encode_ctx_object_id(&mut buf, 0, oid);
        let mut pos = 0;
        let decoded = decode_ctx_object_id(&buf, &mut pos, 0).unwrap();
        assert_eq!(decoded, oid);
    }
}
