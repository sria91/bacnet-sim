/// MS/TP master node state machine.
///
/// Implements the token-passing master per ASHRAE 135-2020 Clause 9.3.

use bacnet_codec::mstp::{MstpFrame, MstpFrameType};
use bytes::Bytes;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::time::timeout;

use super::virtual_link::VirtualMstpBus;

/// Subset of the MS/TP master state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MasterState {
    Initialize,
    Idle,
    UseToken,
    WaitForReply,
    PassToken,
    DoneWithToken,
    PollForMaster,
    NoToken,
}

pub struct VirtualMstpMaster {
    pub address: u8,
    pub max_master: u8,
    state: MasterState,
    next_station: u8,
    token_count: u8,
    bus_tx: broadcast::Sender<MstpFrame>,
    bus_rx: broadcast::Receiver<MstpFrame>,
    token_received_count: Arc<AtomicU64>,
}

/// A received data frame.
pub struct ReceivedData {
    pub source: u8,
    pub data: Bytes,
}

impl VirtualMstpMaster {
    pub fn join(bus: &VirtualMstpBus, address: u8, max_master: u8) -> Self {
        Self {
            address,
            max_master,
            state: MasterState::Initialize,
            next_station: (address + 1) % (max_master + 1),
            token_count: 0,
            bus_tx: bus.sender(),
            bus_rx: bus.subscribe(),
            token_received_count: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Returns a shared counter for test assertions.
    pub fn token_received_count(&self) -> Arc<AtomicU64> {
        self.token_received_count.clone()
    }

    /// Wait until this node receives the token (or `deadline` elapses).
    pub async fn wait_for_token(&mut self, deadline: Duration) -> Result<(), ()> {
        let result = timeout(deadline, async {
            loop {
                if let Ok(frame) = self.bus_rx.recv().await {
                    if frame.frame_type == MstpFrameType::Token
                        && frame.destination == self.address
                    {
                        self.token_received_count.fetch_add(1, Ordering::Relaxed);
                        return;
                    }
                }
            }
        })
        .await;
        result.map_err(|_| ())
    }

    /// Send a data frame to `destination`.
    pub async fn send_data(
        &self,
        destination: u8,
        data: Bytes,
        expecting_reply: bool,
    ) -> Result<(), ()> {
        let frame_type = if expecting_reply {
            MstpFrameType::BacnetDataExpectingReply
        } else {
            MstpFrameType::BacnetDataNotExpectingReply
        };
        let frame = MstpFrame {
            frame_type,
            destination,
            source: self.address,
            data,
        };
        self.bus_tx.send(frame).map(|_| ()).map_err(|_| ())
    }

    /// Receive the next data frame addressed to this node.
    pub async fn recv_data(&mut self) -> Option<ReceivedData> {
        loop {
            if let Ok(frame) = self.bus_rx.recv().await {
                if frame.destination == self.address
                    && matches!(
                        frame.frame_type,
                        MstpFrameType::BacnetDataExpectingReply
                            | MstpFrameType::BacnetDataNotExpectingReply
                    )
                {
                    return Some(ReceivedData {
                        source: frame.source,
                        data: frame.data,
                    });
                }
            }
        }
    }

    /// Pass the token to the next station.
    pub fn pass_token(&self) {
        let frame = MstpFrame {
            frame_type: MstpFrameType::Token,
            destination: self.next_station,
            source: self.address,
            data: Bytes::new(),
        };
        let _ = self.bus_tx.send(frame);
    }
}
