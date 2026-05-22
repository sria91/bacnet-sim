use crate::error::BacnetError;
use crate::property_value::PropertyValue;
/// Application-tagged value encoder/decoder (high-level wrappers).
///
/// These wrap the lower-level [`super::asn1`] primitives to encode/decode
/// full [`crate::property_value::PropertyValue`] instances.
use bytes::BytesMut;

pub fn encode_property_value(buf: &mut BytesMut, value: &PropertyValue) -> Result<(), BacnetError> {
    use super::asn1::*;
    match value {
        PropertyValue::Null => buf.extend_from_slice(&[0x00]),
        PropertyValue::Boolean(v) => buf.extend_from_slice(&[0x11, if *v { 0x01 } else { 0x00 }]),
        PropertyValue::Unsigned(v) => encode_application_unsigned(buf, *v),
        PropertyValue::Real(v) => encode_application_real(buf, *v),
        PropertyValue::CharacterString(s) => {
            let bytes = s.as_bytes();
            // Tag 7, UTF-8 encoding byte (0x00) + string bytes
            let len = bytes.len() + 1;
            buf.extend_from_slice(&[0x75, len as u8, 0x00]);
            buf.extend_from_slice(bytes);
        }
        PropertyValue::ObjectId(oid) => encode_application_object_id(buf, *oid),
        PropertyValue::Date(d) => encode_application_date(buf, *d),
        PropertyValue::BitString(b) => encode_application_bitstring(buf, b),
        PropertyValue::Enumerated(v) => {
            let (len, bytes) = {
                if *v < 0x100 {
                    (1u8, [*v as u8, 0, 0, 0])
                } else {
                    (4, v.to_be_bytes().into())
                }
            };
            buf.extend_from_slice(&[0x90 | len]);
            buf.extend_from_slice(&bytes[..len as usize]);
        }
        PropertyValue::Any(raw) => buf.extend_from_slice(raw),
        // Array and List: encode each element in sequence within the enclosing
        // context tag (the [3E]/[3F] wrapper is written by the caller).
        PropertyValue::Array(items) | PropertyValue::List(items) => {
            for item in items {
                encode_property_value(buf, item)?;
            }
        }
        PropertyValue::Integer(v) => {
            // App tag 3, always 4-byte 2's-complement big-endian (simplest correct form)
            buf.extend_from_slice(&[0x34]);
            buf.extend_from_slice(&v.to_be_bytes());
        }
        PropertyValue::Double(v) => {
            // App tag 5, extended-length 8 bytes (IEEE 754 double)
            buf.extend_from_slice(&[0x55, 0x08]);
            buf.extend_from_slice(&v.to_be_bytes());
        }
        PropertyValue::OctetString(bytes) => {
            // App tag 6
            let len = bytes.len();
            if len <= 4 {
                buf.extend_from_slice(&[0x60 | len as u8]);
            } else {
                buf.extend_from_slice(&[0x65, len as u8]);
            }
            buf.extend_from_slice(bytes);
        }
        PropertyValue::Time(t) => {
            // App tag 11, length 4: hour, minute, second, hundredths
            buf.extend_from_slice(&[0xB4, t.hour, t.minute, t.second, t.hundredths]);
        }
    }
    Ok(())
}
