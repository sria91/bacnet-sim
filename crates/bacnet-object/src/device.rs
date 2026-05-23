/// Device object (ASHRAE 135-2020 §12.11).
use bacnet_types::{
    error::BacnetError,
    property_value::{BacnetDate, BacnetTime, BitString, Weekday},
    DeviceId, ObjectId, ObjectType, PropertyIdentifier, PropertyValue,
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::property::BacnetObject;

pub struct DeviceObject {
    pub device_id: DeviceId,
    pub object_name: String,
    pub vendor_name: String,
    pub vendor_identifier: u16,
    pub model_name: String,
    pub firmware_revision: String,
    pub application_software_version: String,
    pub description: String,
    pub max_apdu_length_accepted: u16,
    pub database_revision: u32,
    pub object_list: Vec<ObjectId>,
}

impl DeviceObject {
    pub fn new(device_id: DeviceId, name: impl Into<String>) -> Self {
        let oid = ObjectId {
            object_type: ObjectType::Device,
            instance: device_id.0,
        };
        Self {
            device_id,
            object_name: name.into(),
            vendor_name: "bacnet-sim".into(),
            vendor_identifier: 999,
            model_name: "bacnet-sim".into(),
            firmware_revision: env!("CARGO_PKG_VERSION").into(),
            application_software_version: env!("CARGO_PKG_VERSION").into(),
            description: String::new(),
            max_apdu_length_accepted: 1476,
            database_revision: 0,
            object_list: vec![oid],
        }
    }
}

impl BacnetObject for DeviceObject {
    fn object_id(&self) -> ObjectId {
        ObjectId {
            object_type: ObjectType::Device,
            instance: self.device_id.0,
        }
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
            PropertyIdentifier::ObjectIdentifier => Ok(PropertyValue::ObjectId(self.object_id())),
            PropertyIdentifier::ObjectName => {
                Ok(PropertyValue::CharacterString(self.object_name.clone()))
            }
            PropertyIdentifier::ObjectType => {
                Ok(PropertyValue::Enumerated(ObjectType::Device as u32))
            }
            PropertyIdentifier::VendorName => {
                Ok(PropertyValue::CharacterString(self.vendor_name.clone()))
            }
            PropertyIdentifier::VendorIdentifier => {
                Ok(PropertyValue::Unsigned(self.vendor_identifier as u32))
            }
            PropertyIdentifier::ModelName => {
                Ok(PropertyValue::CharacterString(self.model_name.clone()))
            }
            PropertyIdentifier::FirmwareRevision => Ok(PropertyValue::CharacterString(
                self.firmware_revision.clone(),
            )),
            PropertyIdentifier::ApplicationSoftwareVersion => Ok(PropertyValue::CharacterString(
                self.application_software_version.clone(),
            )),
            PropertyIdentifier::ProtocolVersion => Ok(PropertyValue::Unsigned(1)),
            PropertyIdentifier::ProtocolRevision => Ok(PropertyValue::Unsigned(22)),
            PropertyIdentifier::MaxApduLengthAccepted => Ok(PropertyValue::Unsigned(
                self.max_apdu_length_accepted as u32,
            )),
            PropertyIdentifier::SegmentationSupported => Ok(PropertyValue::Enumerated(3)), // no-segmentation
            PropertyIdentifier::DatabaseRevision => {
                Ok(PropertyValue::Unsigned(self.database_revision))
            }
            PropertyIdentifier::SystemStatus => Ok(PropertyValue::Enumerated(0)), // operational
            PropertyIdentifier::Description => {
                Ok(PropertyValue::CharacterString(self.description.clone()))
            }
            PropertyIdentifier::ObjectList => match array_index {
                Some(0) => Ok(PropertyValue::Unsigned(self.object_list.len() as u32)),
                Some(i) => {
                    let idx = (i as usize).saturating_sub(1);
                    self.object_list
                        .get(idx)
                        .map(|&oid| PropertyValue::ObjectId(oid))
                        .ok_or(BacnetError::ValueOutOfRange)
                }
                None => Ok(PropertyValue::Array(
                    self.object_list
                        .iter()
                        .map(|&oid| PropertyValue::ObjectId(oid))
                        .collect(),
                )),
            },
            PropertyIdentifier::ApduTimeout => Ok(PropertyValue::Unsigned(6000)),
            PropertyIdentifier::NumberOfApduRetries => Ok(PropertyValue::Unsigned(3)),
            PropertyIdentifier::ProtocolServicesSupported => {
                // 40-bit BitString per ASHRAE 135-2020 Table 12-11.
                // Bit indices: subscribeCOV=5, readProperty=12,
                // readPropertyMultiple=14, writeProperty=15, i-Am=26, who-Is=34
                let mut bits = vec![false; 40];
                bits[5] = true; // subscribeCOV
                bits[12] = true; // readProperty
                bits[14] = true; // readPropertyMultiple
                bits[15] = true; // writeProperty
                bits[26] = true; // i-Am
                bits[34] = true; // who-Is
                Ok(PropertyValue::BitString(BitString::from_bits(&bits)))
            }
            PropertyIdentifier::ProtocolObjectTypesSupported => {
                // 32-bit BitString per ASHRAE 135-2020 Table 12-12.
                // Bit indices: analog-input=0, binary-input=3, device=8
                let mut bits = vec![false; 32];
                bits[0] = true; // analog-input
                bits[3] = true; // binary-input
                bits[8] = true; // device
                Ok(PropertyValue::BitString(BitString::from_bits(&bits)))
            }
            PropertyIdentifier::LocalDate => {
                let (date, _) = current_utc();
                Ok(PropertyValue::Date(date))
            }
            PropertyIdentifier::LocalTime => {
                let (_, time) = current_utc();
                Ok(PropertyValue::Time(time))
            }
            PropertyIdentifier::UtcOffset => Ok(PropertyValue::Integer(0)),
            PropertyIdentifier::DaylightSavingsStatus => Ok(PropertyValue::Boolean(false)),
            PropertyIdentifier::PropertyList => {
                // ASHRAE 135 §12.11.7 — all implemented properties except
                // ObjectIdentifier, ObjectName, ObjectType, and PropertyList itself.
                let props: &[u32] = &[
                    112, // SystemStatus
                    121, // VendorName
                    120, // VendorIdentifier
                    70,  // ModelName
                    44,  // FirmwareRevision
                    12,  // ApplicationSoftwareVersion
                    98,  // ProtocolVersion
                    139, // ProtocolRevision
                    97,  // ProtocolServicesSupported
                    96,  // ProtocolObjectTypesSupported
                    76,  // ObjectList
                    62,  // MaxApduLengthAccepted
                    107, // SegmentationSupported
                    11,  // ApduTimeout
                    73,  // NumberOfApduRetries
                    155, // DatabaseRevision
                    28,  // Description
                    56,  // LocalDate
                    57,  // LocalTime
                    119, // UtcOffset
                    24,  // DaylightSavingsStatus
                    371, // PropertyList (this property — included per standard)
                ];
                match array_index {
                    Some(0) => Ok(PropertyValue::Unsigned(props.len() as u32)),
                    Some(i) => props
                        .get((i as usize).saturating_sub(1))
                        .map(|&id| PropertyValue::Enumerated(id))
                        .ok_or(BacnetError::ValueOutOfRange),
                    None => Ok(PropertyValue::Array(
                        props
                            .iter()
                            .map(|&id| PropertyValue::Enumerated(id))
                            .collect(),
                    )),
                }
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

    fn tick(&mut self, _now: SystemTime, _delta: Duration) {}

    fn changed_since(&self, _since: Instant) -> Vec<PropertyIdentifier> {
        vec![]
    }

    fn all_properties(&self) -> Vec<(PropertyIdentifier, PropertyValue)> {
        vec![
            (
                PropertyIdentifier::ObjectIdentifier,
                PropertyValue::ObjectId(self.object_id()),
            ),
            (
                PropertyIdentifier::ObjectName,
                PropertyValue::CharacterString(self.object_name.clone()),
            ),
            (
                PropertyIdentifier::ObjectType,
                PropertyValue::Enumerated(ObjectType::Device as u32),
            ),
            (
                PropertyIdentifier::VendorName,
                PropertyValue::CharacterString(self.vendor_name.clone()),
            ),
            (
                PropertyIdentifier::VendorIdentifier,
                PropertyValue::Unsigned(self.vendor_identifier as u32),
            ),
            (
                PropertyIdentifier::ModelName,
                PropertyValue::CharacterString(self.model_name.clone()),
            ),
            (
                PropertyIdentifier::FirmwareRevision,
                PropertyValue::CharacterString(self.firmware_revision.clone()),
            ),
            (
                PropertyIdentifier::ProtocolVersion,
                PropertyValue::Unsigned(1),
            ),
            (
                PropertyIdentifier::ProtocolRevision,
                PropertyValue::Unsigned(22),
            ),
            (
                PropertyIdentifier::MaxApduLengthAccepted,
                PropertyValue::Unsigned(self.max_apdu_length_accepted as u32),
            ),
            (
                PropertyIdentifier::SegmentationSupported,
                PropertyValue::Enumerated(3),
            ),
            (
                PropertyIdentifier::DatabaseRevision,
                PropertyValue::Unsigned(self.database_revision),
            ),
            (
                PropertyIdentifier::SystemStatus,
                PropertyValue::Enumerated(0),
            ),
            (
                PropertyIdentifier::Description,
                PropertyValue::CharacterString(self.description.clone()),
            ),
            (
                PropertyIdentifier::ApduTimeout,
                PropertyValue::Unsigned(6000),
            ),
            (
                PropertyIdentifier::NumberOfApduRetries,
                PropertyValue::Unsigned(3),
            ),
            (
                PropertyIdentifier::ObjectList,
                PropertyValue::Array(
                    self.object_list
                        .iter()
                        .map(|&oid| PropertyValue::ObjectId(oid))
                        .collect(),
                ),
            ),
            {
                let mut bits = vec![false; 40];
                bits[5] = true;
                bits[12] = true;
                bits[14] = true;
                bits[15] = true;
                bits[26] = true;
                bits[34] = true;
                (
                    PropertyIdentifier::ProtocolServicesSupported,
                    PropertyValue::BitString(BitString::from_bits(&bits)),
                )
            },
            {
                let mut bits = vec![false; 32];
                bits[0] = true;
                bits[3] = true;
                bits[8] = true;
                (
                    PropertyIdentifier::ProtocolObjectTypesSupported,
                    PropertyValue::BitString(BitString::from_bits(&bits)),
                )
            },
        ]
    }
}

// ---------------------------------------------------------------------------
// UTC time helpers (no external crate required)
// ---------------------------------------------------------------------------

fn is_leap(y: u32) -> bool {
    (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400)
}

/// Return the current UTC date and time derived from `SystemTime::now()`.
fn current_utc() -> (BacnetDate, BacnetTime) {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let time = BacnetTime {
        hour: ((secs / 3600) % 24) as u8,
        minute: ((secs / 60) % 60) as u8,
        second: (secs % 60) as u8,
        hundredths: 0,
    };

    // Jan 1 1970 was a Thursday (index 3 counting from Monday=0).
    let weekday = match (secs / 86400 + 3) % 7 {
        0 => Weekday::Monday,
        1 => Weekday::Tuesday,
        2 => Weekday::Wednesday,
        3 => Weekday::Thursday,
        4 => Weekday::Friday,
        5 => Weekday::Saturday,
        _ => Weekday::Sunday,
    };

    let mut days = (secs / 86400) as u32;
    let mut year = 1970u32;
    loop {
        let diy = if is_leap(year) { 366 } else { 365 };
        if days < diy {
            break;
        }
        days -= diy;
        year += 1;
    }
    let month_lens = [
        31u32,
        if is_leap(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 1u8;
    for (i, &ml) in month_lens.iter().enumerate() {
        if days < ml {
            month = (i + 1) as u8;
            break;
        }
        days -= ml;
    }
    let date = BacnetDate {
        year: year as u16,
        month,
        day: (days + 1) as u8,
        weekday,
    };
    (date, time)
}
