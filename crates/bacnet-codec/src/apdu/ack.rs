use bacnet_types::encoding::{
    application::encode_property_value,
    tags::{encode_closing_tag, encode_ctx_object_id, encode_ctx_u32, encode_opening_tag},
};
use bacnet_types::{error::BacnetError, ObjectId, PropertyIdentifier, PropertyValue};
use bytes::{BufMut, BytesMut};

/// ReadProperty ACK payload.
#[derive(Debug, Clone, PartialEq)]
pub struct ReadPropertyAck {
    pub object_id: ObjectId,
    pub property_id: PropertyIdentifier,
    pub array_index: Option<u32>,
    pub value: PropertyValue,
}

/// ComplexACK service response union.
#[derive(Debug, Clone, PartialEq)]
pub enum ComplexAckService {
    ReadProperty(ReadPropertyAck),
    ReadPropertyMultiple(Vec<ObjectPropertyResult>),
}

/// Per-object result in an RPM response.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectPropertyResult {
    pub object_id: ObjectId,
    pub property_results: Vec<PropertyResult>,
}

/// Individual property result within an RPM response.
#[derive(Debug, Clone, PartialEq)]
pub struct PropertyResult {
    pub property_id: PropertyIdentifier,
    pub array_index: Option<u32>,
    pub value: Result<PropertyValue, BacnetError>,
}

/// A BACnet ComplexACK PDU.
#[derive(Debug, Clone, PartialEq)]
pub struct ComplexAck {
    pub invoke_id: u8,
    pub service: ComplexAckService,
}

impl ComplexAck {
    /// Encode the full ComplexACK PDU including service body.
    ///
    /// ReadProperty-ACK wire layout (after the 3-byte PDU header):
    /// ```text
    /// [0] object-identifier   (context 0, 4 bytes)
    /// [1] property-identifier (context 1, 1-4 bytes)
    /// [2] array-index         (context 2, optional)
    /// [3E] property-value [3F]
    /// ```
    ///
    /// ReadPropertyMultiple-ACK wire layout:
    /// ```text
    /// repeat per object:
    ///   [0] object-identifier
    ///   [1E]
    ///     repeat per property:
    ///       [2] property-identifier
    ///       [3] array-index (optional)
    ///       [4E] value [4F]  OR  [5E] error-class + error-code [5F]
    ///   [1F]
    /// ```
    pub fn encode(&self, buf: &mut BytesMut) {
        buf.put_u8(0x30); // PDU type ComplexACK
        buf.put_u8(self.invoke_id);
        match &self.service {
            ComplexAckService::ReadProperty(ack) => {
                buf.put_u8(12); // service choice
                encode_read_property_ack(buf, ack);
            }
            ComplexAckService::ReadPropertyMultiple(results) => {
                buf.put_u8(14); // service choice
                encode_rpm_ack(buf, results);
            }
        }
    }

    pub fn decode(buf: &[u8]) -> Result<Self, BacnetError> {
        if buf.len() < 3 {
            return Err(BacnetError::DecodeError("complex ACK too short".into()));
        }
        if (buf[0] & 0xF0) != 0x30 {
            return Err(BacnetError::DecodeError(format!(
                "expected ComplexACK, got {:#02x}",
                buf[0]
            )));
        }
        Ok(Self {
            invoke_id: buf[1],
            service: ComplexAckService::ReadProperty(ReadPropertyAck {
                object_id: ObjectId {
                    object_type: bacnet_types::ObjectType::Device,
                    instance: 0,
                },
                property_id: PropertyIdentifier::ObjectIdentifier,
                array_index: None,
                value: PropertyValue::Null,
            }),
        })
    }
}

fn encode_read_property_ack(buf: &mut BytesMut, ack: &ReadPropertyAck) {
    // [0] object-identifier
    encode_ctx_object_id(buf, 0, ack.object_id);
    // [1] property-identifier
    encode_ctx_u32(buf, 1, ack.property_id.to_u32());
    // [2] array-index (optional)
    if let Some(idx) = ack.array_index {
        encode_ctx_u32(buf, 2, idx);
    }
    // [3E] value [3F]
    encode_opening_tag(buf, 3);
    let _ = encode_property_value(buf, &ack.value);
    encode_closing_tag(buf, 3);
}

fn encode_rpm_ack(buf: &mut BytesMut, results: &[ObjectPropertyResult]) {
    for obj_result in results {
        // [0] object-identifier
        encode_ctx_object_id(buf, 0, obj_result.object_id);
        // [1E] ... [1F]
        encode_opening_tag(buf, 1);
        for prop in &obj_result.property_results {
            // [2] property-identifier
            encode_ctx_u32(buf, 2, prop.property_id.to_u32());
            // [3] array-index (optional)
            if let Some(idx) = prop.array_index {
                encode_ctx_u32(buf, 3, idx);
            }
            match &prop.value {
                Ok(pv) => {
                    // [4E] value [4F]
                    encode_opening_tag(buf, 4);
                    let _ = encode_property_value(buf, pv);
                    encode_closing_tag(buf, 4);
                }
                Err(e) => {
                    // [5E] error-class + error-code [5F]
                    encode_opening_tag(buf, 5);
                    let (ec, code) = error_class_code(e);
                    encode_ctx_u32(buf, 0, ec);
                    encode_ctx_u32(buf, 1, code);
                    encode_closing_tag(buf, 5);
                }
            }
        }
        encode_closing_tag(buf, 1);
    }
}

/// Map a `BacnetError` to `(error_class, error_code)` numeric values.
fn error_class_code(e: &BacnetError) -> (u32, u32) {
    match e {
        BacnetError::UnknownObject => (1, 31), // Object / UnknownObject
        BacnetError::UnknownProperty => (2, 32), // Property / UnknownProperty
        BacnetError::WriteAccessDenied => (2, 40), // Property / WriteAccessDenied
        BacnetError::ValueOutOfRange => (2, 37), // Property / ValueOutOfRange
        BacnetError::InvalidDataType => (2, 9), // Property / InvalidDataType
        BacnetError::ServiceError {
            error_class,
            error_code,
        } => {
            use bacnet_types::error::{ErrorClass, ErrorCode};
            let ec: u32 = match error_class {
                ErrorClass::Device => 0,
                ErrorClass::Object => 1,
                ErrorClass::Property => 2,
                ErrorClass::Resources => 3,
                ErrorClass::Security => 4,
                ErrorClass::Services => 5,
                ErrorClass::Vt => 6,
                ErrorClass::Communication => 7,
            };
            let code: u32 = match error_code {
                ErrorCode::UnknownObject => 31,
                ErrorCode::UnknownProperty => 32,
                ErrorCode::WriteAccessDenied => 40,
                ErrorCode::ValueOutOfRange => 37,
                ErrorCode::InvalidDataType => 9,
                ErrorCode::ServiceRequestDenied => 29,
                ErrorCode::NotConfigured => 140,
                ErrorCode::Other(v) => *v,
            };
            (ec, code)
        }
        _ => (5, 0), // Services / Other
    }
}

/// A BACnet SimpleACK PDU.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimpleAck {
    pub invoke_id: u8,
    pub service_choice: u8,
}

impl SimpleAck {
    pub fn encode(&self, buf: &mut BytesMut) {
        buf.put_u8(0x20); // SimpleACK
        buf.put_u8(self.invoke_id);
        buf.put_u8(self.service_choice);
    }
}
