/// Multi-State Input object (ASHRAE 135-2020 §12.14) — read-only enumerated sensor.

use bacnet_types::{
    DeviceId, ObjectId, ObjectType, PropertyIdentifier, PropertyValue,
    property_value::{EventState, Reliability, StatusFlags},
    error::BacnetError,
};
use std::time::Instant;

use crate::property::BacnetObject;

pub struct MultiStateInput {
    pub device_id:       DeviceId,
    pub object_id:       ObjectId,
    pub object_name:     String,
    pub description:     String,
    pub present_value:   u32,  // 1-based state number
    pub number_of_states: u32,
    pub state_text:      Vec<String>,
    pub out_of_service:  bool,
    pub status_flags:    StatusFlags,
    pub event_state:     EventState,
    pub reliability:     Reliability,
    pub(crate) last_changed: Instant,
}

impl MultiStateInput {
    pub fn new(device_id: DeviceId, instance: u32, name: impl Into<String>, number_of_states: u32) -> Self {
        Self {
            device_id,
            object_id: ObjectId { object_type: ObjectType::MultiStateInput, instance },
            object_name: name.into(),
            description: String::new(),
            present_value: 1,
            number_of_states,
            state_text: (1..=number_of_states).map(|i| format!("State {i}")).collect(),
            out_of_service: false,
            status_flags: StatusFlags::default(),
            event_state: EventState::Normal,
            reliability: Reliability::NoFaultDetected,
            last_changed: Instant::now(),
        }
    }
}

impl BacnetObject for MultiStateInput {
    fn object_id(&self) -> ObjectId { self.object_id }
    fn device_id(&self) -> DeviceId { self.device_id }

    fn read_property(&self, property_id: PropertyIdentifier, _array_index: Option<u32>) -> Result<PropertyValue, BacnetError> {
        match property_id {
            PropertyIdentifier::ObjectIdentifier  => Ok(PropertyValue::ObjectId(self.object_id)),
            PropertyIdentifier::ObjectName        => Ok(PropertyValue::CharacterString(self.object_name.clone())),
            PropertyIdentifier::ObjectType        => Ok(PropertyValue::Enumerated(ObjectType::MultiStateInput as u32)),
            PropertyIdentifier::Description       => Ok(PropertyValue::CharacterString(self.description.clone())),
            PropertyIdentifier::PresentValue      => Ok(PropertyValue::Unsigned(self.present_value)),
            PropertyIdentifier::NumberOfStates    => Ok(PropertyValue::Unsigned(self.number_of_states)),
            PropertyIdentifier::StatusFlags       => Ok(PropertyValue::BitString(status_flags_bits(&self.status_flags))),
            PropertyIdentifier::EventState        => Ok(PropertyValue::Enumerated(self.event_state as u32)),
            PropertyIdentifier::Reliability       => Ok(PropertyValue::Enumerated(self.reliability as u32)),
            PropertyIdentifier::OutOfService      => Ok(PropertyValue::Boolean(self.out_of_service)),
            _ => Err(BacnetError::UnknownProperty),
        }
    }

    fn write_property(&mut self, property_id: PropertyIdentifier, _array_index: Option<u32>, value: PropertyValue, _priority: Option<u8>) -> Result<(), BacnetError> {
        match property_id {
            PropertyIdentifier::OutOfService => {
                if let PropertyValue::Boolean(v) = value { self.out_of_service = v; Ok(()) }
                else { Err(BacnetError::InvalidDataType) }
            }
            PropertyIdentifier::PresentValue if self.out_of_service => {
                if let PropertyValue::Unsigned(v) = value {
                    if v >= 1 && v <= self.number_of_states {
                        self.present_value = v;
                        self.last_changed = Instant::now();
                        Ok(())
                    } else {
                        Err(BacnetError::ValueOutOfRange)
                    }
                } else { Err(BacnetError::InvalidDataType) }
            }
            _ => Err(BacnetError::WriteAccessDenied),
        }
    }

    fn tick(&mut self, _now: std::time::SystemTime, _delta: std::time::Duration) {}
    fn changed_since(&self, since: Instant) -> Vec<PropertyIdentifier> {
        if self.last_changed > since { vec![PropertyIdentifier::PresentValue] } else { vec![] }
    }
    fn all_properties(&self) -> Vec<(PropertyIdentifier, PropertyValue)> {
        vec![
            (PropertyIdentifier::ObjectIdentifier, PropertyValue::ObjectId(self.object_id)),
            (PropertyIdentifier::ObjectName, PropertyValue::CharacterString(self.object_name.clone())),
            (PropertyIdentifier::ObjectType, PropertyValue::Enumerated(ObjectType::MultiStateInput as u32)),
            (PropertyIdentifier::Description, PropertyValue::CharacterString(self.description.clone())),
            (PropertyIdentifier::PresentValue, PropertyValue::Unsigned(self.present_value)),
            (PropertyIdentifier::NumberOfStates, PropertyValue::Unsigned(self.number_of_states)),
            (PropertyIdentifier::StatusFlags, PropertyValue::BitString(status_flags_bits(&self.status_flags))),
            (PropertyIdentifier::EventState, PropertyValue::Enumerated(self.event_state as u32)),
            (PropertyIdentifier::Reliability, PropertyValue::Enumerated(self.reliability as u32)),
            (PropertyIdentifier::OutOfService, PropertyValue::Boolean(self.out_of_service)),
        ]
    }
}

fn status_flags_bits(sf: &StatusFlags) -> bacnet_types::property_value::BitString {
    bacnet_types::property_value::BitString::from_bits(&[sf.in_alarm, sf.fault, sf.overridden, sf.out_of_service])
}
