/// BBMD (BACnet Broadcast Management Device) support.
///
/// Stub — full BDT/FDT management implemented in later phases.
use std::net::SocketAddrV4;

pub struct BdtTable(pub Vec<BdtEntry>);
pub struct FdtTable(pub Vec<FdtEntry>);

pub struct BdtEntry {
    pub address: SocketAddrV4,
    pub mask: [u8; 4],
}

pub struct FdtEntry {
    pub address: SocketAddrV4,
    pub ttl: u16,
    pub remaining: u16,
}

impl BdtTable {
    pub fn new() -> Self {
        Self(Vec::new())
    }
}

impl FdtTable {
    pub fn new() -> Self {
        Self(Vec::new())
    }
}
