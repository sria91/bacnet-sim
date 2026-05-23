/// Schedule object (ASHRAE 135-2020 §12.16).
/// Simplified: weekly time-value schedule with per-device-property output list.
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bacnet_types::{
    error::BacnetError, DeviceId, ObjectId, ObjectType, PropertyIdentifier, PropertyValue,
};

use crate::property::BacnetObject;

/// A single time→value slot within a daily schedule.
#[derive(Debug, Clone)]
pub struct TimeValue {
    /// Seconds from midnight (0..86400).
    pub time_of_day_secs: u32,
    pub value: PropertyValue,
}

/// Schedule for one day of the week.
#[derive(Debug, Clone, Default)]
pub struct DailySchedule {
    pub slots: Vec<TimeValue>,
}

impl DailySchedule {
    /// Return the active value at the given seconds-from-midnight, or `None`
    /// if no slot has started yet (caller should use `schedule_default`).
    pub fn active_at(&self, time_of_day_secs: u32) -> Option<&PropertyValue> {
        let mut active: Option<&PropertyValue> = None;
        for slot in &self.slots {
            if time_of_day_secs >= slot.time_of_day_secs {
                active = Some(&slot.value);
            }
        }
        active
    }
}

pub struct Schedule {
    pub device_id: DeviceId,
    pub object_id: ObjectId,
    pub object_name: String,
    pub description: String,
    pub present_value: PropertyValue,
    pub out_of_service: bool,
    /// Monday=0 … Sunday=6.
    pub weekly_schedule: [DailySchedule; 7],
    pub schedule_default: PropertyValue,
    /// Objects/properties written when present_value changes.
    pub list_of_object_property_references: Vec<(ObjectId, PropertyIdentifier)>,
}

impl Schedule {
    pub fn new(device_id: DeviceId, instance: u32, name: impl Into<String>) -> Self {
        Self {
            device_id,
            object_id: ObjectId {
                object_type: ObjectType::Schedule,
                instance,
            },
            object_name: name.into(),
            description: String::new(),
            present_value: PropertyValue::Null,
            out_of_service: false,
            weekly_schedule: Default::default(),
            schedule_default: PropertyValue::Null,
            list_of_object_property_references: Vec::new(),
        }
    }

    /// Evaluate the schedule at `secs_since_epoch` and return the active value.
    pub fn evaluate_at(&self, secs_since_epoch: u64) -> &PropertyValue {
        // Day-of-week: epoch day 0 is Thursday. Adjust so 0=Monday.
        let day_of_week = ((secs_since_epoch / 86_400 + 3) % 7) as usize;
        let time_of_day = (secs_since_epoch % 86_400) as u32;
        self.weekly_schedule[day_of_week]
            .active_at(time_of_day)
            .unwrap_or(&self.schedule_default)
    }
}

impl BacnetObject for Schedule {
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
                Ok(PropertyValue::Enumerated(ObjectType::Schedule as u32))
            }
            PropertyIdentifier::Description => {
                Ok(PropertyValue::CharacterString(self.description.clone()))
            }
            PropertyIdentifier::PresentValue => Ok(self.present_value.clone()),
            PropertyIdentifier::OutOfService => Ok(PropertyValue::Boolean(self.out_of_service)),
            PropertyIdentifier::ScheduleDefault => Ok(self.schedule_default.clone()),
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
                if self.out_of_service {
                    self.present_value = value;
                    Ok(())
                } else {
                    Err(BacnetError::WriteAccessDenied)
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
            PropertyIdentifier::ScheduleDefault => {
                self.schedule_default = value;
                Ok(())
            }
            _ => Err(BacnetError::WriteAccessDenied),
        }
    }

    fn tick(&mut self, now: SystemTime, _delta: Duration) {
        if self.out_of_service {
            return;
        }
        let secs = now.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        self.present_value = self.evaluate_at(secs).clone();
    }

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
                PropertyValue::Enumerated(ObjectType::Schedule as u32),
            ),
            (
                PropertyIdentifier::Description,
                PropertyValue::CharacterString(self.description.clone()),
            ),
            (PropertyIdentifier::PresentValue, self.present_value.clone()),
            (
                PropertyIdentifier::OutOfService,
                PropertyValue::Boolean(self.out_of_service),
            ),
            (
                PropertyIdentifier::ScheduleDefault,
                self.schedule_default.clone(),
            ),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_schedule() -> Schedule {
        let mut sched = Schedule::new(DeviceId(1), 1, "SCHED-01");
        sched.schedule_default = PropertyValue::Real(18.0);
        // Monday: occupied 08:00→21.0, unoccupied 18:00→18.0
        sched.weekly_schedule[0].slots = vec![
            TimeValue {
                time_of_day_secs: 8 * 3600,
                value: PropertyValue::Real(21.0),
            },
            TimeValue {
                time_of_day_secs: 18 * 3600,
                value: PropertyValue::Real(18.0),
            },
        ];
        sched
    }

    #[test]
    fn schedule_default_before_first_slot() {
        let sched = make_schedule();
        // epoch day 3 (Thursday, but mapping: (3+3)%7=6=Sunday); use a known Monday
        // 2024-01-01 00:00:00 UTC = 1704067200, which is a Monday
        // Before 08:00 -> schedule_default
        let val = sched.evaluate_at(1_704_067_200 + 7 * 3600); // 07:00
        assert_eq!(val, &PropertyValue::Real(18.0));
    }

    #[test]
    fn schedule_occupied_slot() {
        let sched = make_schedule();
        // 2024-01-01 09:00 UTC (Monday)
        let val = sched.evaluate_at(1_704_067_200 + 9 * 3600);
        assert_eq!(val, &PropertyValue::Real(21.0));
    }

    #[test]
    fn schedule_unoccupied_slot() {
        let sched = make_schedule();
        // 2024-01-01 19:00 UTC (Monday)
        let val = sched.evaluate_at(1_704_067_200 + 19 * 3600);
        assert_eq!(val, &PropertyValue::Real(18.0));
    }
}
