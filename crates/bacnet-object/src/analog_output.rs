/// Analog Output object (ASHRAE 135-2020 §12.3).
use bacnet_types::{
    error::BacnetError,
    property_value::{EngineeringUnits, EventState, Reliability, StatusFlags},
    DeviceId, ObjectId, ObjectType, PropertyIdentifier, PropertyValue,
};
use std::time::{Duration, Instant, SystemTime};

use crate::property::BacnetObject;

const NUM_PRIORITIES: usize = 16;

pub struct AnalogOutput {
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
    pub priority_array: [Option<f32>; NUM_PRIORITIES],
    pub relinquish_default: f32,
    pub cov_increment: f32,

    pub(crate) last_changed: Instant,
}

impl AnalogOutput {
    pub fn new(
        device_id: DeviceId,
        instance: u32,
        name: impl Into<String>,
        units: EngineeringUnits,
    ) -> Self {
        Self {
            device_id,
            object_id: ObjectId {
                object_type: ObjectType::AnalogOutput,
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
            priority_array: [None; NUM_PRIORITIES],
            relinquish_default: 0.0,
            cov_increment: 0.1,
            last_changed: Instant::now(),
        }
    }

    fn effective_value(&self) -> f32 {
        for &pv in &self.priority_array {
            if let Some(v) = pv {
                return v;
            }
        }
        self.relinquish_default
    }
}

impl BacnetObject for AnalogOutput {
    fn object_id(&self) -> ObjectId {
        self.object_id
    }
    fn device_id(&self) -> DeviceId {
        self.device_id
    }

    fn read_property(
        &self,
        property_id: PropertyIdentifier,
        array_index: Option<u32>,
    ) -> Result<PropertyValue, BacnetError> {
        match property_id {
            PropertyIdentifier::ObjectIdentifier => Ok(PropertyValue::ObjectId(self.object_id)),
            PropertyIdentifier::ObjectName => {
                Ok(PropertyValue::CharacterString(self.object_name.clone()))
            }
            PropertyIdentifier::ObjectType => {
                Ok(PropertyValue::Enumerated(ObjectType::AnalogOutput as u32))
            }
            PropertyIdentifier::PresentValue => Ok(PropertyValue::Real(self.effective_value())),
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
            PropertyIdentifier::Units => Ok(PropertyValue::Enumerated(self.units as u32)),
            PropertyIdentifier::RelinquishDefault => {
                Ok(PropertyValue::Real(self.relinquish_default))
            }
            PropertyIdentifier::PriorityArray => {
                if let Some(idx) = array_index {
                    let i = (idx as usize).saturating_sub(1).min(NUM_PRIORITIES - 1);
                    Ok(self.priority_array[i]
                        .map(PropertyValue::Real)
                        .unwrap_or(PropertyValue::Null))
                } else {
                    let arr = self
                        .priority_array
                        .iter()
                        .map(|v| v.map(PropertyValue::Real).unwrap_or(PropertyValue::Null))
                        .collect();
                    Ok(PropertyValue::Array(arr))
                }
            }
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
        priority: Option<u8>,
    ) -> Result<(), BacnetError> {
        match property_id {
            PropertyIdentifier::PresentValue => {
                let pri = priority.unwrap_or(16) as usize;
                if !(1..=16).contains(&pri) {
                    return Err(BacnetError::ValueOutOfRange);
                }
                let idx = pri - 1;
                if value == PropertyValue::Null {
                    self.priority_array[idx] = None; // relinquish
                } else {
                    self.priority_array[idx] =
                        Some(value.as_f32().ok_or(BacnetError::InvalidDataType)?);
                }
                self.present_value = self.effective_value();
                self.last_changed = Instant::now();
                Ok(())
            }
            PropertyIdentifier::OutOfService => {
                self.out_of_service = value.as_bool().ok_or(BacnetError::InvalidDataType)?;
                Ok(())
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
                PropertyValue::Enumerated(ObjectType::AnalogOutput as u32),
            ),
            (
                PropertyIdentifier::PresentValue,
                PropertyValue::Real(self.effective_value()),
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
                PropertyIdentifier::RelinquishDefault,
                PropertyValue::Real(self.relinquish_default),
            ),
            (
                PropertyIdentifier::Description,
                PropertyValue::CharacterString(self.description.clone()),
            ),
        ]
    }
}
