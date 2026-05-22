/// BACnet/SC node — client-side WebSocket connection.
///
/// Stub implementation for Phase 5.

use bacnet_types::ScNodeId;
use super::hub::ScConnectionState;

pub struct ScNode {
    pub node_id: ScNodeId,
    pub vmac: [u8; 6],
    state: ScConnectionState,
}

impl ScNode {
    pub fn new(node_id: ScNodeId, vmac: [u8; 6]) -> Self {
        Self { node_id, vmac, state: ScConnectionState::Disconnected }
    }

    pub fn connection_state(&self) -> ScConnectionState {
        self.state
    }

    pub fn vmac(&self) -> [u8; 6] {
        self.vmac
    }
}
