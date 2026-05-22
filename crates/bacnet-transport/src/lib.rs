pub mod bbmd;
pub mod ip;
pub mod mstp;
pub mod sc;

use bacnet_types::NetworkAddress;
use bytes::Bytes;

/// A frame received from any transport, normalised for the APDU dispatcher.
#[derive(Debug, Clone)]
pub struct InboundFrame {
    pub src: NetworkAddress,
    pub npdu: Bytes,
}

/// A frame to be sent out through any transport.
#[derive(Debug, Clone)]
pub struct OutboundFrame {
    pub dst: Destination,
    pub npdu: Bytes,
}

#[derive(Debug, Clone)]
pub enum Destination {
    Unicast(NetworkAddress),
    Broadcast { network_number: u16 },
}
