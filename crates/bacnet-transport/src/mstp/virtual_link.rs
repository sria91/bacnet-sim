/// In-process virtual MS/TP bus.
///
/// All simulated MS/TP nodes share this bus via broadcast channels.
use bacnet_codec::mstp::MstpFrame;
use tokio::sync::broadcast;

const BUS_CHANNEL_CAPACITY: usize = 512;

/// Shared virtual bus — clone to give each node access.
#[derive(Clone)]
pub struct VirtualMstpBus {
    tx: broadcast::Sender<MstpFrame>,
}

impl VirtualMstpBus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(BUS_CHANNEL_CAPACITY);
        Self { tx }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<MstpFrame> {
        self.tx.subscribe()
    }

    pub fn sender(&self) -> broadcast::Sender<MstpFrame> {
        self.tx.clone()
    }
}

impl Default for VirtualMstpBus {
    fn default() -> Self {
        Self::new()
    }
}
