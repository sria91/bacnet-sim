use crate::ObjectId;
use serde::{Deserialize, Serialize};

/// BACnet engineering units (ASHRAE 135-2020 §23.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[repr(u16)]
pub enum EngineeringUnits {
    #[default]
    NoUnits = 95,
    DegreesCelsius = 62,
    DegreesFahrenheit = 64,
    Kelvin = 63,
    Percent = 98,
    PoundsPerSquareInch = 6,
    CubicMetersPerSecond = 19,
    Liters = 82,
    Watts = 47,
    Kilowatts = 48,
    Amperes = 2,
    Volts = 5,
    Hertz = 27,
    Meters = 31,
}

/// BACnet event state (ASHRAE 135-2020 §21.2.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EventState {
    #[default]
    Normal = 0,
    Fault = 1,
    OffNormal = 2,
    HighLimit = 3,
    LowLimit = 4,
    LifeSafetyAlarm = 5,
}

/// BACnet reliability (ASHRAE 135-2020 §21.2.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Reliability {
    #[default]
    NoFaultDetected = 0,
    NoSensor = 1,
    OverRange = 2,
    UnderRange = 3,
    OpenLoop = 4,
    ShortedLoop = 5,
    NoOutput = 6,
    Unreliable = 7,
    ProcessError = 8,
    MultiStateFault = 9,
    ConfigurationError = 10,
    CommunicationFailure = 12,
    MemberFault = 13,
    MonitoredObjectFault = 14,
    Tripped = 15,
    LampFailure = 16,
    ActivationFailure = 17,
    RenewDhcpFailure = 18,
    RenewFdRegistrationFailure = 19,
    RestartAutoNegotiationFailure = 20,
    RestartFailure = 21,
    ProprietaryCommandFailure = 22,
    FaultsListed = 23,
    ReferencedObjectFault = 24,
}

/// Status flags bit-set: [in-alarm, fault, overridden, out-of-service].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StatusFlags {
    pub in_alarm: bool,
    pub fault: bool,
    pub overridden: bool,
    pub out_of_service: bool,
}

/// A variable-length bit string.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BitString {
    data: Vec<bool>,
}

impl BitString {
    pub fn from_bits(bits: &[bool]) -> Self {
        Self {
            data: bits.to_vec(),
        }
    }

    pub fn bits(&self) -> &[bool] {
        &self.data
    }
}

/// BACnet calendar date.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BacnetDate {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub weekday: Weekday,
}

/// BACnet time of day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BacnetTime {
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub hundredths: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Weekday {
    Monday = 1,
    Tuesday = 2,
    Wednesday = 3,
    Thursday = 4,
    Friday = 5,
    Saturday = 6,
    Sunday = 7,
    Unspecified = 255,
}

/// The universal BACnet property value container.
#[derive(Debug, Clone, PartialEq)]
pub enum PropertyValue {
    Null,
    Boolean(bool),
    Unsigned(u32),
    Integer(i32),
    Real(f32),
    Double(f64),
    OctetString(bytes::Bytes),
    CharacterString(String),
    BitString(BitString),
    Enumerated(u32),
    Date(BacnetDate),
    Time(BacnetTime),
    ObjectId(ObjectId),
    Array(Vec<PropertyValue>),
    List(Vec<PropertyValue>),
    /// Raw encoded bytes — pass-through for opaque values.
    Any(bytes::Bytes),
}

impl PropertyValue {
    /// Attempt to interpret as an f32 (Real or Double).
    pub fn as_f32(&self) -> Option<f32> {
        match self {
            Self::Real(v) => Some(*v),
            Self::Double(v) => Some(*v as f32),
            Self::Unsigned(v) => Some(*v as f32),
            Self::Integer(v) => Some(*v as f32),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Boolean(v) => Some(*v),
            Self::Enumerated(v) => Some(*v != 0),
            _ => None,
        }
    }
}

impl From<f32> for PropertyValue {
    fn from(v: f32) -> Self {
        Self::Real(v)
    }
}

impl From<bool> for PropertyValue {
    fn from(v: bool) -> Self {
        Self::Boolean(v)
    }
}

impl From<u32> for PropertyValue {
    fn from(v: u32) -> Self {
        Self::Unsigned(v)
    }
}

impl From<String> for PropertyValue {
    fn from(v: String) -> Self {
        Self::CharacterString(v)
    }
}
