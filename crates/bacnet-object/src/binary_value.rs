/// Binary Value object (ASHRAE 135-2020 §12.8) — a command-able boolean value.
use bacnet_types::{
    error::BacnetError,
    property_value::{EventState, Reliability, StatusFlags},
    DeviceId, ObjectId, ObjectType, PropertyIdentifier, PropertyValue,
};
use std::time::Instant;

use crate::property::BacnetObject;

pub struct BinaryValue {
    pub device_id: DeviceId,
    pub object_id: ObjectId,
    pub object_name: String,
    pub description: String,
    pub present_value: bool,
    pub out_of_service: bool,
    pub status_flags: StatusFlags,
    pub event_state: EventState,
    pub reliability: Reliability,
    pub(crate) last_changed: Instant,
}

impl BinaryValue {
    pub fn new(device_id: DeviceId, instance: u32, name: impl Into<String>) -> Self {
        Self {
            device_id,
            object_id: ObjectId {
                object_type: ObjectType::BinaryValue,
                instance,
            },
            object_name: name.into(),
            description: String::new(),
            present_value: false,
            out_of_service: false,
            status_flags: StatusFlags::default(),
            event_state: EventState::Normal,
            reliability: Reliability::NoFaultDetected,
            last_changed: Instant::now(),
        }
    }
}

impl BacnetObject for BinaryValue {
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
            PropertyIdentifier::ObjectType => {
                Ok(PropertyValue::Enumerated(ObjectType::BinaryValue as u32))
            }
            PropertyIdentifier::Description => {
                Ok(PropertyValue::CharacterString(self.description.clone()))
            }
            PropertyIdentifier::PresentValue => Ok(PropertyValue::Boolean(self.present_value)),
            PropertyIdentifier::StatusFlags => Ok(PropertyValue::BitString(status_flags_bits(
                &self.status_flags,
            ))),
            PropertyIdentifier::EventState => {
                Ok(PropertyValue::Enumerated(self.event_state as u32))
            }
            PropertyIdentifier::Reliability => {
                Ok(PropertyValue::Enumerated(self.reliability as u32))
            }
            PropertyIdentifier::OutOfService => Ok(PropertyValue::Boolean(self.out_of_service)),
            _ => Err(BacnetError::UnknownProperty),
        }
    }

    fn write_property(
        &mut self,
        property_id: PropertyIdentifier,
        _array_index: Option<u32>,
        value: PropertyValue,
        _priority: Option<u8>,
    ) -> Result<(), BacnetError> {
        match property_id {
            PropertyIdentifier::PresentValue => match value {
                PropertyValue::Boolean(v) => {
                    self.present_value = v;
                    self.last_changed = Instant::now();
                    Ok(())
                }
                PropertyValue::Enumerated(v) | PropertyValue::Unsigned(v) => {
                    self.present_value = v != 0;
                    self.last_changed = Instant::now();
                    Ok(())
                }
                _ => Err(BacnetError::InvalidDataType),
            },
            PropertyIdentifier::OutOfService => {
                if let PropertyValue::Boolean(v) = value {
                    self.out_of_service = v;
                    Ok(())
                } else {
                    Err(BacnetError::InvalidDataType)
                }
            }
            _ => Err(BacnetError::WriteAccessDenied),
        }
    }

    fn tick(&mut self, _now: std::time::SystemTime, _delta: std::time::Duration) {}
    fn changed_since(&self, since: Instant) -> Vec<PropertyIdentifier> {
        if self.last_changed > since {
            vec![PropertyIdentifier::PresentValue]
        } else {
            vec![]
        }
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
                PropertyValue::Enumerated(ObjectType::BinaryValue as u32),
            ),
            (
                PropertyIdentifier::Description,
                PropertyValue::CharacterString(self.description.clone()),
            ),
            (
                PropertyIdentifier::PresentValue,
                PropertyValue::Boolean(self.present_value),
            ),
            (
                PropertyIdentifier::StatusFlags,
                PropertyValue::BitString(status_flags_bits(&self.status_flags)),
            ),
            (
                PropertyIdentifier::EventState,
                PropertyValue::Enumerated(self.event_state as u32),
            ),
            (
                PropertyIdentifier::Reliability,
                PropertyValue::Enumerated(self.reliability as u32),
            ),
            (
                PropertyIdentifier::OutOfService,
                PropertyValue::Boolean(self.out_of_service),
            ),
        ]
    }
}

fn status_flags_bits(sf: &StatusFlags) -> bacnet_types::property_value::BitString {
    bacnet_types::property_value::BitString::from_bits(&[
        sf.in_alarm,
        sf.fault,
        sf.overridden,
        sf.out_of_service,
    ])
}
