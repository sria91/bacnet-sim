/// Analog Value object (ASHRAE 135-2020 §12.4).
/// Like AnalogInput but PresentValue is always writable (no out-of-service guard).
use bacnet_types::{
    error::BacnetError,
    property_value::{EngineeringUnits, EventState, Reliability, StatusFlags},
    DeviceId, ObjectId, ObjectType, PropertyIdentifier, PropertyValue,
};
use std::time::Instant;

use crate::property::BacnetObject;

pub struct AnalogValue {
    pub device_id: DeviceId,
    pub object_id: ObjectId,
    pub object_name: String,
    pub description: String,
    pub present_value: f32,
    pub status_flags: StatusFlags,
    pub event_state: EventState,
    pub reliability: Reliability,
    pub out_of_service: bool,
    pub units: EngineeringUnits,
    pub cov_increment: f32,
    pub(crate) last_changed: Instant,
}

impl AnalogValue {
    pub fn new(
        device_id: DeviceId,
        instance: u32,
        name: impl Into<String>,
        units: EngineeringUnits,
    ) -> Self {
        Self {
            device_id,
            object_id: ObjectId {
                object_type: ObjectType::AnalogValue,
                instance,
            },
            object_name: name.into(),
            description: String::new(),
            present_value: 0.0,
            status_flags: StatusFlags::default(),
            event_state: EventState::Normal,
            reliability: Reliability::NoFaultDetected,
            out_of_service: false,
            units,
            cov_increment: 0.1,
            last_changed: Instant::now(),
        }
    }
}

impl BacnetObject for AnalogValue {
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
                Ok(PropertyValue::Enumerated(ObjectType::AnalogValue as u32))
            }
            PropertyIdentifier::Description => {
                Ok(PropertyValue::CharacterString(self.description.clone()))
            }
            PropertyIdentifier::PresentValue => Ok(PropertyValue::Real(self.present_value)),
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
            PropertyIdentifier::Units => Ok(PropertyValue::Enumerated(self.units as u32)),
            PropertyIdentifier::CovIncrement => Ok(PropertyValue::Real(self.cov_increment)),
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
            PropertyIdentifier::PresentValue => {
                if let PropertyValue::Real(v) = value {
                    self.present_value = v;
                    self.last_changed = Instant::now();
                    Ok(())
                } else {
                    Err(BacnetError::InvalidDataType)
                }
            }
            PropertyIdentifier::OutOfService => {
                if let PropertyValue::Boolean(v) = value {
                    self.out_of_service = v;
                    Ok(())
                } else {
                    Err(BacnetError::InvalidDataType)
                }
            }
            PropertyIdentifier::Description => {
                if let PropertyValue::CharacterString(s) = value {
                    self.description = s;
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
                PropertyValue::Enumerated(ObjectType::AnalogValue as u32),
            ),
            (
                PropertyIdentifier::Description,
                PropertyValue::CharacterString(self.description.clone()),
            ),
            (
                PropertyIdentifier::PresentValue,
                PropertyValue::Real(self.present_value),
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
            (
                PropertyIdentifier::Units,
                PropertyValue::Enumerated(self.units as u32),
            ),
            (
                PropertyIdentifier::CovIncrement,
                PropertyValue::Real(self.cov_increment),
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
