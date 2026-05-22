/// Device object (ASHRAE 135-2020 §12.11).

use bacnet_types::{
    DeviceId, ObjectId, ObjectType, PropertyIdentifier, PropertyValue,
    error::BacnetError,
};
use std::time::{Duration, Instant, SystemTime};

use crate::property::BacnetObject;

pub struct DeviceObject {
    pub device_id: DeviceId,
    pub object_name: String,
    pub vendor_name: String,
    pub vendor_identifier: u16,
    pub model_name: String,
    pub firmware_revision: String,
    pub application_software_version: String,
    pub description: String,
    pub max_apdu_length_accepted: u16,
    pub database_revision: u32,
    pub object_list: Vec<ObjectId>,
}

impl DeviceObject {
    pub fn new(device_id: DeviceId, name: impl Into<String>) -> Self {
        let oid = ObjectId {
            object_type: ObjectType::Device,
            instance: device_id.0,
        };
        Self {
            device_id,
            object_name: name.into(),
            vendor_name: "bacnet-sim".into(),
            vendor_identifier: 999,
            model_name: "bacnet-sim".into(),
            firmware_revision: env!("CARGO_PKG_VERSION").into(),
            application_software_version: env!("CARGO_PKG_VERSION").into(),
            description: String::new(),
            max_apdu_length_accepted: 1476,
            database_revision: 0,
            object_list: vec![oid],
        }
    }
}

impl BacnetObject for DeviceObject {
    fn object_id(&self) -> ObjectId {
        ObjectId { object_type: ObjectType::Device, instance: self.device_id.0 }
    }

    fn device_id(&self) -> DeviceId { self.device_id }

    fn read_property(
        &self,
        property_id: PropertyIdentifier,
        array_index: Option<u32>,
    ) -> Result<PropertyValue, BacnetError> {
        match property_id {
            PropertyIdentifier::ObjectIdentifier =>
                Ok(PropertyValue::ObjectId(self.object_id())),
            PropertyIdentifier::ObjectName =>
                Ok(PropertyValue::CharacterString(self.object_name.clone())),
            PropertyIdentifier::ObjectType =>
                Ok(PropertyValue::Enumerated(ObjectType::Device as u32)),
            PropertyIdentifier::VendorName =>
                Ok(PropertyValue::CharacterString(self.vendor_name.clone())),
            PropertyIdentifier::VendorIdentifier =>
                Ok(PropertyValue::Unsigned(self.vendor_identifier as u32)),
            PropertyIdentifier::ModelName =>
                Ok(PropertyValue::CharacterString(self.model_name.clone())),
            PropertyIdentifier::FirmwareRevision =>
                Ok(PropertyValue::CharacterString(self.firmware_revision.clone())),
            PropertyIdentifier::ApplicationSoftwareVersion =>
                Ok(PropertyValue::CharacterString(self.application_software_version.clone())),
            PropertyIdentifier::ProtocolVersion =>
                Ok(PropertyValue::Unsigned(1)),
            PropertyIdentifier::ProtocolRevision =>
                Ok(PropertyValue::Unsigned(22)),
            PropertyIdentifier::MaxApduLengthAccepted =>
                Ok(PropertyValue::Unsigned(self.max_apdu_length_accepted as u32)),
            PropertyIdentifier::SegmentationSupported =>
                Ok(PropertyValue::Enumerated(3)), // no-segmentation
            PropertyIdentifier::DatabaseRevision =>
                Ok(PropertyValue::Unsigned(self.database_revision)),
            PropertyIdentifier::SystemStatus =>
                Ok(PropertyValue::Enumerated(0)), // operational
            PropertyIdentifier::Description =>
                Ok(PropertyValue::CharacterString(self.description.clone())),
            PropertyIdentifier::ObjectList => {
                match array_index {
                    Some(0) => Ok(PropertyValue::Unsigned(self.object_list.len() as u32)),
                    Some(i) => {
                        let idx = (i as usize).saturating_sub(1);
                        self.object_list.get(idx)
                            .map(|&oid| PropertyValue::ObjectId(oid))
                            .ok_or(BacnetError::ValueOutOfRange)
                    }
                    None => Ok(PropertyValue::Array(
                        self.object_list.iter().map(|&oid| PropertyValue::ObjectId(oid)).collect()
                    )),
                }
            }
            PropertyIdentifier::ApduTimeout =>
                Ok(PropertyValue::Unsigned(6000)),
            PropertyIdentifier::NumberOfApduRetries =>
                Ok(PropertyValue::Unsigned(3)),
            _ => Err(BacnetError::UnknownProperty),
        }
    }

    fn write_property(
        &mut self,
        _property_id: PropertyIdentifier,
        _array_index: Option<u32>,
        _value: PropertyValue,
        _priority: Option<u8>,
    ) -> Result<(), BacnetError> {
        Err(BacnetError::WriteAccessDenied)
    }

    fn tick(&mut self, _now: SystemTime, _delta: Duration) {}

    fn changed_since(&self, _since: Instant) -> Vec<PropertyIdentifier> {
        vec![]
    }

    fn all_properties(&self) -> Vec<(PropertyIdentifier, PropertyValue)> {
        vec![
            (PropertyIdentifier::ObjectIdentifier, PropertyValue::ObjectId(self.object_id())),
            (PropertyIdentifier::ObjectName, PropertyValue::CharacterString(self.object_name.clone())),
            (PropertyIdentifier::ObjectType, PropertyValue::Enumerated(ObjectType::Device as u32)),
            (PropertyIdentifier::VendorName, PropertyValue::CharacterString(self.vendor_name.clone())),
            (PropertyIdentifier::VendorIdentifier, PropertyValue::Unsigned(self.vendor_identifier as u32)),
            (PropertyIdentifier::ModelName, PropertyValue::CharacterString(self.model_name.clone())),
            (PropertyIdentifier::FirmwareRevision, PropertyValue::CharacterString(self.firmware_revision.clone())),
            (PropertyIdentifier::ProtocolVersion, PropertyValue::Unsigned(1)),
            (PropertyIdentifier::ProtocolRevision, PropertyValue::Unsigned(22)),
            (PropertyIdentifier::MaxApduLengthAccepted, PropertyValue::Unsigned(self.max_apdu_length_accepted as u32)),
            (PropertyIdentifier::SegmentationSupported, PropertyValue::Enumerated(3)),
            (PropertyIdentifier::DatabaseRevision, PropertyValue::Unsigned(self.database_revision)),
            (PropertyIdentifier::SystemStatus, PropertyValue::Enumerated(0)),
        ]
    }
}
