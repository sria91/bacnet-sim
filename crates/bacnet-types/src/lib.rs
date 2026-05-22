pub mod application_tags;
pub mod encoding;
pub mod error;
pub mod object_types;
pub mod property_id;
pub mod property_value;

pub use error::BacnetError;
pub use object_types::ObjectType;
pub use property_id::PropertyIdentifier;
pub use property_value::PropertyValue;

/// 22-bit BACnet object instance number (max 0x3FFFFF).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ObjectId {
    pub object_type: ObjectType,
    pub instance: u32,
}

/// 22-bit BACnet device instance number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeviceId(pub u32);

/// BACnet network address (network number + MAC).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NetworkAddress {
    pub network_number: u16, // 0 = local
    pub mac: MacAddr,
}

/// MAC address variants across the three supported transports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MacAddr {
    Ip(std::net::SocketAddrV4),
    MsTP(u8),
    Sc(ScNodeId),
}

/// BACnet/SC node identifier (128-bit UUID).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScNodeId(pub [u8; 16]);

impl ScNodeId {
    pub fn random() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        // Deterministic placeholder — replace with uuid crate in production.
        let t = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos();
        let mut id = [0u8; 16];
        id[..4].copy_from_slice(&t.to_le_bytes());
        Self(id)
    }
}
