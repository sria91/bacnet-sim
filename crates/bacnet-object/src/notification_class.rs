/// Notification Class object (ASHRAE 135-2020 §12.23) — alarm routing configuration.
use bacnet_types::{
    error::BacnetError, DeviceId, ObjectId, ObjectType, PropertyIdentifier, PropertyValue,
};
use std::time::Instant;

use crate::property::BacnetObject;

pub struct NotificationClass {
    pub device_id: DeviceId,
    pub object_id: ObjectId,
    pub object_name: String,
    pub description: String,
    /// Priority for [to-offnormal, to-fault, to-normal]
    pub priority: [u8; 3],
    /// Which transitions require explicit acknowledgement
    pub ack_required: [bool; 3],
    pub notification_class: u32,
}

impl NotificationClass {
    pub fn new(device_id: DeviceId, instance: u32, name: impl Into<String>) -> Self {
        Self {
            device_id,
            object_id: ObjectId {
                object_type: ObjectType::NotificationClass,
                instance,
            },
            object_name: name.into(),
            description: String::new(),
            priority: [111, 111, 111],
            ack_required: [false, false, false],
            notification_class: instance,
        }
    }
}

impl BacnetObject for NotificationClass {
    fn object_id(&self) -> ObjectId {
        self.object_id
    }
    fn device_id(&self) -> DeviceId {
        self.device_id
    }

    fn read_property(
        &self,
        property_id: PropertyIdentifier,
        _array_index: Option<u32>,
    ) -> Result<PropertyValue, BacnetError> {
        match property_id {
            PropertyIdentifier::ObjectIdentifier => Ok(PropertyValue::ObjectId(self.object_id)),
            PropertyIdentifier::ObjectName => {
                Ok(PropertyValue::CharacterString(self.object_name.clone()))
            }
            PropertyIdentifier::ObjectType => Ok(PropertyValue::Enumerated(
                ObjectType::NotificationClass as u32,
            )),
            PropertyIdentifier::Description => {
                Ok(PropertyValue::CharacterString(self.description.clone()))
            }
            PropertyIdentifier::NotificationClass => {
                Ok(PropertyValue::Unsigned(self.notification_class))
            }
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

    fn tick(&mut self, _now: std::time::SystemTime, _delta: std::time::Duration) {}
    fn changed_since(&self, _since: Instant) -> Vec<PropertyIdentifier> {
        vec![]
    }
    fn all_properties(&self) -> Vec<(PropertyIdentifier, PropertyValue)> {
        vec![
            (
                PropertyIdentifier::ObjectIdentifier,
                PropertyValue::ObjectId(self.object_id),
            ),
            (
                PropertyIdentifier::ObjectName,
                PropertyValue::CharacterString(self.object_name.clone()),
            ),
            (
                PropertyIdentifier::ObjectType,
                PropertyValue::Enumerated(ObjectType::NotificationClass as u32),
            ),
            (
                PropertyIdentifier::Description,
                PropertyValue::CharacterString(self.description.clone()),
            ),
            (
                PropertyIdentifier::NotificationClass,
                PropertyValue::Unsigned(self.notification_class),
            ),
        ]
    }
}
