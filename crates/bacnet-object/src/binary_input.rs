/// Binary Input object (ASHRAE 135-2020 §12.6).
use bacnet_types::{
    error::BacnetError,
    property_value::{EventState, Reliability, StatusFlags},
    DeviceId, ObjectId, ObjectType, PropertyIdentifier, PropertyValue,
};
use std::time::{Duration, Instant, SystemTime};

use crate::property::BacnetObject;

pub struct BinaryInput {
    pub device_id: DeviceId,
    pub object_id: ObjectId,
    pub object_name: String,
    pub description: String,
    pub present_value: bool,
    pub status_flags: StatusFlags,
    pub event_state: EventState,
    pub reliability: Reliability,
    pub out_of_service: bool,
    pub polarity: bool, // false=Normal, true=Reverse

    pub(crate) last_changed: Instant,
}

impl BinaryInput {
    pub fn new(device_id: DeviceId, instance: u32, name: impl Into<String>) -> Self {
        Self {
            device_id,
            object_id: ObjectId {
                object_type: ObjectType::BinaryInput,
                instance,
            },
            object_name: name.into(),
            description: String::new(),
            present_value: false,
            status_flags: StatusFlags::default(),
            event_state: EventState::Normal,
            reliability: Reliability::NoFaultDetected,
            out_of_service: false,
            polarity: false,
            last_changed: Instant::now(),
        }
    }
}

impl BacnetObject for BinaryInput {
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
                Ok(PropertyValue::Enumerated(ObjectType::BinaryInput as u32))
            }
            PropertyIdentifier::PresentValue => {
                Ok(PropertyValue::Enumerated(self.present_value as u32))
            }
            PropertyIdentifier::StatusFlags => Ok(PropertyValue::BitString(
                bacnet_types::property_value::BitString::from_bits(&[
                    self.status_flags.in_alarm,
                    self.status_flags.fault,
                    self.status_flags.overridden,
                    self.status_flags.out_of_service,
                ]),
            )),
            PropertyIdentifier::EventState => {
                Ok(PropertyValue::Enumerated(self.event_state as u32))
            }
            PropertyIdentifier::OutOfService => Ok(PropertyValue::Boolean(self.out_of_service)),
            PropertyIdentifier::Description => {
                Ok(PropertyValue::CharacterString(self.description.clone()))
            }
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
            PropertyIdentifier::OutOfService => {
                self.out_of_service = value.as_bool().ok_or(BacnetError::InvalidDataType)?;
                Ok(())
            }
            PropertyIdentifier::PresentValue if self.out_of_service => {
                self.present_value = value.as_bool().ok_or(BacnetError::InvalidDataType)?;
                self.last_changed = Instant::now();
                Ok(())
            }
            PropertyIdentifier::PresentValue => Err(BacnetError::WriteAccessDenied),
            _ => Err(BacnetError::WriteAccessDenied),
        }
    }

    fn tick(&mut self, _now: SystemTime, _delta: Duration) {}

    fn changed_since(&self, since: Instant) -> Vec<PropertyIdentifier> {
        if self.last_changed >= since {
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
                PropertyValue::Enumerated(ObjectType::BinaryInput as u32),
            ),
            (
                PropertyIdentifier::PresentValue,
                PropertyValue::Enumerated(self.present_value as u32),
            ),
            (
                PropertyIdentifier::OutOfService,
                PropertyValue::Boolean(self.out_of_service),
            ),
            (
                PropertyIdentifier::Description,
                PropertyValue::CharacterString(self.description.clone()),
            ),
        ]
    }
}
