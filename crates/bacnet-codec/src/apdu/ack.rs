use bacnet_types::{ObjectId, PropertyIdentifier, PropertyValue, error::BacnetError};
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
    pub fn encode(&self, buf: &mut BytesMut) {
        buf.put_u8(0x30); // PDU type ComplexACK
        buf.put_u8(self.invoke_id);
        let choice = match &self.service {
            ComplexAckService::ReadProperty(_) => 12u8,
            ComplexAckService::ReadPropertyMultiple(_) => 14,
        };
        buf.put_u8(choice);
    }

    pub fn decode(buf: &[u8]) -> Result<Self, BacnetError> {
        if buf.len() < 3 {
            return Err(BacnetError::DecodeError("complex ACK too short".into()));
        }
        if (buf[0] & 0xF0) != 0x30 {
            return Err(BacnetError::DecodeError(format!("expected ComplexACK, got {:#02x}", buf[0])));
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
