/// Analog Input object (ASHRAE 135-2020 §12.2).

use bacnet_types::{
    DeviceId, ObjectId, ObjectType, PropertyIdentifier, PropertyValue,
    property_value::{EngineeringUnits, EventState, Reliability, StatusFlags},
    error::{BacnetError, ErrorClass, ErrorCode},
};
use std::time::{Duration, Instant, SystemTime};

use crate::property::BacnetObject;

pub struct AnalogInput {
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
    pub min_present_value: Option<f32>,
    pub max_present_value: Option<f32>,
    pub cov_increment: f32,
    pub time_delay: u32,
    pub notification_class: Option<u32>,
    pub high_limit: Option<f32>,
    pub low_limit: Option<f32>,
    pub deadband: f32,
    pub profile_name: Option<String>,

    pub(crate) last_cov_val: f32,
    pub(crate) last_changed: Instant,
}

impl AnalogInput {
    pub fn new(device_id: DeviceId, instance: u32, name: impl Into<String>, units: EngineeringUnits) -> Self {
        Self {
            device_id,
            object_id: ObjectId { object_type: ObjectType::AnalogInput, instance },
            object_name: name.into(),
            description: String::new(),
            present_value: 0.0,
            status_flags: StatusFlags::default(),
            event_state: EventState::Normal,
            reliability: Reliability::NoFaultDetected,
            out_of_service: false,
            units,
            min_present_value: None,
            max_present_value: None,
            cov_increment: 0.1,
            time_delay: 0,
            notification_class: None,
            high_limit: None,
            low_limit: None,
            deadband: 0.0,
            profile_name: None,
            last_cov_val: 0.0,
            last_changed: Instant::now(),
        }
    }
}

impl BacnetObject for AnalogInput {
    fn object_id(&self) -> ObjectId { self.object_id }
    fn device_id(&self) -> DeviceId { self.device_id }

    fn read_property(
        &self,
        property_id: PropertyIdentifier,
        _array_index: Option<u32>,
    ) -> Result<PropertyValue, BacnetError> {
        match property_id {
            PropertyIdentifier::ObjectIdentifier =>
                Ok(PropertyValue::ObjectId(self.object_id)),
            PropertyIdentifier::ObjectName =>
                Ok(PropertyValue::CharacterString(self.object_name.clone())),
            PropertyIdentifier::ObjectType =>
                Ok(PropertyValue::Enumerated(ObjectType::AnalogInput as u32)),
            PropertyIdentifier::PresentValue =>
                Ok(PropertyValue::Real(self.present_value)),
            PropertyIdentifier::StatusFlags =>
                Ok(PropertyValue::BitString(status_flags_bits(&self.status_flags))),
            PropertyIdentifier::EventState =>
                Ok(PropertyValue::Enumerated(self.event_state as u32)),
            PropertyIdentifier::Reliability =>
                Ok(PropertyValue::Enumerated(self.reliability as u32)),
            PropertyIdentifier::OutOfService =>
                Ok(PropertyValue::Boolean(self.out_of_service)),
            PropertyIdentifier::Units =>
                Ok(PropertyValue::Enumerated(self.units as u32)),
            PropertyIdentifier::Description =>
                Ok(PropertyValue::CharacterString(self.description.clone())),
            PropertyIdentifier::CovIncrement =>
                Ok(PropertyValue::Real(self.cov_increment)),
            PropertyIdentifier::HighLimit =>
                self.high_limit.map(PropertyValue::Real)
                    .ok_or(BacnetError::UnknownProperty),
            PropertyIdentifier::LowLimit =>
                self.low_limit.map(PropertyValue::Real)
                    .ok_or(BacnetError::UnknownProperty),
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
            PropertyIdentifier::PresentValue => {
                if !self.out_of_service {
                    return Err(BacnetError::WriteAccessDenied);
                }
                self.present_value = value.as_f32().ok_or(BacnetError::InvalidDataType)?;
                self.last_changed = Instant::now();
                Ok(())
            }
            PropertyIdentifier::CovIncrement => {
                self.cov_increment = value.as_f32().ok_or(BacnetError::InvalidDataType)?;
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

    fn tick(&mut self, _now: SystemTime, _delta: Duration) {
        // Value model drives present_value; called by SimEngine.
    }

    fn changed_since(&self, since: Instant) -> Vec<PropertyIdentifier> {
        if self.last_changed >= since {
            vec![PropertyIdentifier::PresentValue, PropertyIdentifier::StatusFlags]
        } else {
            vec![]
        }
    }

    fn all_properties(&self) -> Vec<(PropertyIdentifier, PropertyValue)> {
        let mut props = vec![
            (PropertyIdentifier::ObjectIdentifier, PropertyValue::ObjectId(self.object_id)),
            (PropertyIdentifier::ObjectName, PropertyValue::CharacterString(self.object_name.clone())),
            (PropertyIdentifier::ObjectType, PropertyValue::Enumerated(ObjectType::AnalogInput as u32)),
            (PropertyIdentifier::PresentValue, PropertyValue::Real(self.present_value)),
            (PropertyIdentifier::StatusFlags, PropertyValue::BitString(status_flags_bits(&self.status_flags))),
            (PropertyIdentifier::EventState, PropertyValue::Enumerated(self.event_state as u32)),
            (PropertyIdentifier::Reliability, PropertyValue::Enumerated(self.reliability as u32)),
            (PropertyIdentifier::OutOfService, PropertyValue::Boolean(self.out_of_service)),
            (PropertyIdentifier::Units, PropertyValue::Enumerated(self.units as u32)),
            (PropertyIdentifier::Description, PropertyValue::CharacterString(self.description.clone())),
            (PropertyIdentifier::CovIncrement, PropertyValue::Real(self.cov_increment)),
        ];
        if let Some(hi) = self.high_limit {
            props.push((PropertyIdentifier::HighLimit, PropertyValue::Real(hi)));
        }
        if let Some(lo) = self.low_limit {
            props.push((PropertyIdentifier::LowLimit, PropertyValue::Real(lo)));
        }
        props
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
