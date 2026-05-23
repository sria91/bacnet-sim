/// Binary Output object (ASHRAE 135-2020 §12.6) with 16-slot command priority array.
use bacnet_types::{
    error::BacnetError,
    property_value::{EventState, Reliability, StatusFlags},
    DeviceId, ObjectId, ObjectType, PropertyIdentifier, PropertyValue,
};
use std::time::Instant;

use crate::property::BacnetObject;

/// Effective present value computed from the priority array.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryPv {
    Inactive = 0,
    Active = 1,
}

pub struct BinaryOutput {
    pub device_id: DeviceId,
    pub object_id: ObjectId,
    pub object_name: String,
    pub description: String,
    pub out_of_service: bool,
    pub status_flags: StatusFlags,
    pub event_state: EventState,
    pub reliability: Reliability,
    pub relinquish_default: BinaryPv,
    /// 16-slot priority array (index 0 = priority 1, highest).
    pub priority_array: [Option<bool>; 16],
    pub(crate) last_changed: Instant,
}

impl BinaryOutput {
    pub fn new(device_id: DeviceId, instance: u32, name: impl Into<String>) -> Self {
        Self {
            device_id,
            object_id: ObjectId {
                object_type: ObjectType::BinaryOutput,
                instance,
            },
            object_name: name.into(),
            description: String::new(),
            out_of_service: false,
            status_flags: StatusFlags::default(),
            event_state: EventState::Normal,
            reliability: Reliability::NoFaultDetected,
            relinquish_default: BinaryPv::Inactive,
            priority_array: [None; 16],
            last_changed: Instant::now(),
        }
    }

    fn effective_value(&self) -> bool {
        self.priority_array
            .iter()
            .find_map(|s| *s)
            .unwrap_or(self.relinquish_default == BinaryPv::Active)
    }
}

impl BacnetObject for BinaryOutput {
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
                Ok(PropertyValue::Enumerated(ObjectType::BinaryOutput as u32))
            }
            PropertyIdentifier::Description => {
                Ok(PropertyValue::CharacterString(self.description.clone()))
            }
            PropertyIdentifier::PresentValue => {
                Ok(PropertyValue::Enumerated(self.effective_value() as u32))
            }
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
        priority: Option<u8>,
    ) -> Result<(), BacnetError> {
        if property_id == PropertyIdentifier::PresentValue {
            let pri = (priority.unwrap_or(16) as usize).saturating_sub(1).min(15);
            match value {
                PropertyValue::Null => {
                    self.priority_array[pri] = None;
                }
                PropertyValue::Enumerated(v) | PropertyValue::Unsigned(v) => {
                    self.priority_array[pri] = Some(v != 0);
                }
                PropertyValue::Boolean(v) => {
                    self.priority_array[pri] = Some(v);
                }
                _ => return Err(BacnetError::InvalidDataType),
            }
            self.last_changed = Instant::now();
            Ok(())
        } else if property_id == PropertyIdentifier::OutOfService {
            if let PropertyValue::Boolean(v) = value {
                self.out_of_service = v;
                Ok(())
            } else {
                Err(BacnetError::InvalidDataType)
            }
        } else {
            Err(BacnetError::WriteAccessDenied)
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
                PropertyValue::Enumerated(ObjectType::BinaryOutput as u32),
            ),
            (
                PropertyIdentifier::Description,
                PropertyValue::CharacterString(self.description.clone()),
            ),
            (
                PropertyIdentifier::PresentValue,
                PropertyValue::Enumerated(self.effective_value() as u32),
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
