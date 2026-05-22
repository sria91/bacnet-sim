/// Application-tagged value encoder/decoder (high-level wrappers).
///
/// These wrap the lower-level [`super::asn1`] primitives to encode/decode
/// full [`crate::property_value::PropertyValue`] instances.

use bytes::BytesMut;
use crate::error::BacnetError;
use crate::property_value::PropertyValue;

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
                if *v < 0x100 { (1u8, [*v as u8, 0, 0, 0]) }
                else { (4, v.to_be_bytes().into()) }
            };
            buf.extend_from_slice(&[0x90 | len]);
            buf.extend_from_slice(&bytes[..len as usize]);
        }
        PropertyValue::Any(raw) => buf.extend_from_slice(raw),
        _ => return Err(BacnetError::EncodeError("unsupported property value type".into())),
    }
    Ok(())
}
