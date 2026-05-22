/// BACnet application tag numbers (ASHRAE 135-2020 §20.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ApplicationTag {
    Null = 0,
    Boolean = 1,
    UnsignedInt = 2,
    SignedInt = 3,
    Real = 4,
    Double = 5,
    OctetString = 6,
    CharacterString = 7,
    BitString = 8,
    Enumerated = 9,
    Date = 10,
    Time = 11,
    BacnetObjectId = 12,
    // 13-14 reserved
}

impl ApplicationTag {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0  => Some(Self::Null),
            1  => Some(Self::Boolean),
            2  => Some(Self::UnsignedInt),
            3  => Some(Self::SignedInt),
            4  => Some(Self::Real),
            5  => Some(Self::Double),
            6  => Some(Self::OctetString),
            7  => Some(Self::CharacterString),
            8  => Some(Self::BitString),
            9  => Some(Self::Enumerated),
            10 => Some(Self::Date),
            11 => Some(Self::Time),
            12 => Some(Self::BacnetObjectId),
            _  => None,
        }
    }
}
