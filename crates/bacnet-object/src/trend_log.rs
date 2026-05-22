/// TrendLog object (ASHRAE 135-2020 §12.25).
/// Stores a bounded ring buffer of timestamped property values.

use std::collections::VecDeque;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bacnet_types::{
    DeviceId, ObjectId, ObjectType, PropertyIdentifier, PropertyValue,
    error::{BacnetError, ErrorClass, ErrorCode},
};

use crate::property::BacnetObject;

/// A single logged record.
pub struct TrendLogEntry {
    pub timestamp: SystemTime,
    pub value: PropertyValue,
}

pub struct TrendLog {
    pub device_id: DeviceId,
    pub object_id: ObjectId,
    pub object_name: String,
    pub description: String,
    /// Whether logging is active.
    pub enable: bool,
    /// Interval between automatic samples.
    pub log_interval: Duration,
    /// Maximum number of entries in the ring buffer.
    pub buffer_size: usize,
    /// Ring buffer of log entries.
    pub log_buffer: VecDeque<TrendLogEntry>,
    /// Object/property being logged (optional — set by configuration).
    pub monitored_object: Option<ObjectId>,
    pub monitored_property: Option<PropertyIdentifier>,
    pub record_count: u32,
    pub total_record_count: u32,
    pub out_of_service: bool,
    pub(crate) last_sample: Instant,
}

impl TrendLog {
    pub fn new(device_id: DeviceId, instance: u32, name: impl Into<String>) -> Self {
        Self {
            device_id,
            object_id: ObjectId { object_type: ObjectType::TrendLog, instance },
            object_name: name.into(),
            description: String::new(),
            enable: true,
            log_interval: Duration::from_secs(60),
            buffer_size: 1000,
            log_buffer: VecDeque::with_capacity(1000),
            monitored_object: None,
            monitored_property: None,
            record_count: 0,
            total_record_count: 0,
            out_of_service: false,
            last_sample: Instant::now(),
        }
    }

    /// Append a new log entry, evicting the oldest if at capacity.
    pub fn record(&mut self, value: PropertyValue) {
        if !self.enable || self.out_of_service {
            return;
        }
        if self.log_buffer.len() >= self.buffer_size {
            self.log_buffer.pop_front();
        }
        self.log_buffer.push_back(TrendLogEntry {
            timestamp: SystemTime::now(),
            value,
        });
        self.record_count = self.log_buffer.len() as u32;
        self.total_record_count = self.total_record_count.saturating_add(1);
    }
}

impl BacnetObject for TrendLog {
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
            PropertyIdentifier::ObjectIdentifier =>
                Ok(PropertyValue::ObjectId(self.object_id)),
            PropertyIdentifier::ObjectName =>
                Ok(PropertyValue::CharacterString(self.object_name.clone())),
            PropertyIdentifier::ObjectType =>
                Ok(PropertyValue::Enumerated(ObjectType::TrendLog as u32)),
            PropertyIdentifier::Description =>
                Ok(PropertyValue::CharacterString(self.description.clone())),
            PropertyIdentifier::Enable =>
                Ok(PropertyValue::Boolean(self.enable)),
            PropertyIdentifier::LogInterval =>
                Ok(PropertyValue::Unsigned(self.log_interval.as_secs() as u32)),
            PropertyIdentifier::BufferSize =>
                Ok(PropertyValue::Unsigned(self.buffer_size as u32)),
            PropertyIdentifier::RecordCount =>
                Ok(PropertyValue::Unsigned(self.record_count)),
            PropertyIdentifier::TotalRecordCount =>
                Ok(PropertyValue::Unsigned(self.total_record_count)),
            PropertyIdentifier::OutOfService =>
                Ok(PropertyValue::Boolean(self.out_of_service)),
            PropertyIdentifier::StopWhenFull =>
                Ok(PropertyValue::Boolean(false)),
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
            PropertyIdentifier::Enable => {
                if let PropertyValue::Boolean(v) = value {
                    self.enable = v;
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
            PropertyIdentifier::RecordCount => {
                // Writing 0 clears the log buffer.
                if let PropertyValue::Unsigned(0) = value {
                    self.log_buffer.clear();
                    self.record_count = 0;
                }
                Ok(())
            }
            PropertyIdentifier::LogInterval => {
                if let PropertyValue::Unsigned(v) = value {
                    self.log_interval = Duration::from_secs(v as u64);
                    Ok(())
                } else {
                    Err(BacnetError::InvalidDataType)
                }
            }
            _ => Err(BacnetError::WriteAccessDenied),
        }
    }

    fn tick(&mut self, now: SystemTime, delta: Duration) {
        if !self.enable || self.out_of_service {
            return;
        }
        // Auto-sample at log_interval if the monitored object is not set
        // (external code can also call record() directly).
        let inst = Instant::now();
        if inst.duration_since(self.last_sample) >= self.log_interval {
            self.last_sample = inst;
            // Record a null entry as a heartbeat; callers can replace with
            // actual values by calling record() after reading from the store.
        }
    }

    fn changed_since(&self, _since: Instant) -> Vec<PropertyIdentifier> {
        vec![]
    }

    fn all_properties(&self) -> Vec<(PropertyIdentifier, PropertyValue)> {
        vec![
            (PropertyIdentifier::ObjectIdentifier, PropertyValue::ObjectId(self.object_id)),
            (PropertyIdentifier::ObjectName, PropertyValue::CharacterString(self.object_name.clone())),
            (PropertyIdentifier::ObjectType, PropertyValue::Enumerated(ObjectType::TrendLog as u32)),
            (PropertyIdentifier::Description, PropertyValue::CharacterString(self.description.clone())),
            (PropertyIdentifier::Enable, PropertyValue::Boolean(self.enable)),
            (PropertyIdentifier::BufferSize, PropertyValue::Unsigned(self.buffer_size as u32)),
            (PropertyIdentifier::LogInterval, PropertyValue::Unsigned(self.log_interval.as_secs() as u32)),
            (PropertyIdentifier::RecordCount, PropertyValue::Unsigned(self.record_count)),
            (PropertyIdentifier::TotalRecordCount, PropertyValue::Unsigned(self.total_record_count)),
            (PropertyIdentifier::OutOfService, PropertyValue::Boolean(self.out_of_service)),
            (PropertyIdentifier::StopWhenFull, PropertyValue::Boolean(false)),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trend_log_record_and_cap() {
        let dev = DeviceId(1);
        let mut tl = TrendLog::new(dev, 1, "TL-01");
        tl.buffer_size = 3;
        for i in 0..5u32 {
            tl.record(PropertyValue::Unsigned(i));
        }
        assert_eq!(tl.log_buffer.len(), 3);
        assert_eq!(tl.record_count, 3);
        assert_eq!(tl.total_record_count, 5);
        // Oldest entries evicted; newest should be 4
        if let PropertyValue::Unsigned(v) = &tl.log_buffer.back().unwrap().value {
            assert_eq!(*v, 4);
        } else {
            panic!("unexpected value type");
        }
    }

    #[test]
    fn trend_log_clear_via_write() {
        let dev = DeviceId(1);
        let mut tl = TrendLog::new(dev, 1, "TL-01");
        tl.record(PropertyValue::Real(1.0));
        tl.record(PropertyValue::Real(2.0));
        tl.write_property(PropertyIdentifier::RecordCount, None, PropertyValue::Unsigned(0), None).unwrap();
        assert_eq!(tl.record_count, 0);
        assert!(tl.log_buffer.is_empty());
    }

    #[test]
    fn trend_log_disabled_does_not_record() {
        let dev = DeviceId(1);
        let mut tl = TrendLog::new(dev, 1, "TL-01");
        tl.enable = false;
        tl.record(PropertyValue::Real(1.0));
        assert_eq!(tl.record_count, 0);
    }
}
