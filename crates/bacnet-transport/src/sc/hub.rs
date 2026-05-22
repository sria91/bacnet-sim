/// BACnet/SC hub — accepts WebSocket connections from SC nodes.
///
/// Stub implementation for Phase 5.

use bacnet_types::ScNodeId;
use dashmap::DashMap;
use std::sync::Arc;

pub struct ScHub {
    pub nodes: Arc<DashMap<ScNodeId, ScNodeConn>>,
}

pub struct ScNodeConn {
    pub node_id: ScNodeId,
    pub vmac: [u8; 6],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScConnectionState {
    Connecting,
    Connected,
    Disconnected,
}

impl ScHub {
    pub fn new() -> Self {
        Self { nodes: Arc::new(DashMap::new()) }
    }

    pub async fn connected_nodes(&self) -> Vec<ScNodeId> {
        self.nodes.iter().map(|e| *e.key()).collect()
    }
}

impl Default for ScHub {
    fn default() -> Self {
        Self::new()
    }
}
