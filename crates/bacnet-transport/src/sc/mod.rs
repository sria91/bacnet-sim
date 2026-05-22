pub mod hub;
pub mod node;

/// Connection state for a BACnet/SC node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScConnectionState {
    Connecting,
    Connected,
    Disconnected,
}
