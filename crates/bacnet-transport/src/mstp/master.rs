/// MS/TP master node state machine.
///
/// Implements the token-passing master per ASHRAE 135-2020 Clause 9.3.
use bacnet_codec::mstp::{MstpFrame, MstpFrameType};
use bytes::Bytes;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};
use tokio::time::timeout;
use tracing::debug;

use super::virtual_link::VirtualMstpBus;

/// How long to wait for a token before declaring ring failure.
/// Per ASHRAE 135: Tno_token = 500 ms (we use 200 ms for virtual bus speed).
const TOKEN_WAIT_MS: u64 = 200;

/// MS/TP master state (simplified subset).
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
    #[allow(dead_code)]
    token_count: u8,
    bus_tx: broadcast::Sender<MstpFrame>,
    bus_rx: broadcast::Receiver<MstpFrame>,
    token_received_count: Arc<AtomicU64>,
    /// Pending outbound NPDU frames: (destination_address, npdu_bytes)
    pending_tx: Option<mpsc::Receiver<(u8, Bytes)>>,
    pending_tx_handle: Option<mpsc::Sender<(u8, Bytes)>>,
}

/// A received data frame.
pub struct ReceivedData {
    pub source: u8,
    pub data: Bytes,
}

impl VirtualMstpMaster {
    pub fn join(bus: &VirtualMstpBus, address: u8, max_master: u8) -> Self {
        let (pending_tx_handle, pending_rx) = mpsc::channel(64);
        Self {
            address,
            max_master,
            state: MasterState::Initialize,
            next_station: (address + 1) % (max_master + 1),
            token_count: 0,
            bus_tx: bus.sender(),
            bus_rx: bus.subscribe(),
            token_received_count: Arc::new(AtomicU64::new(0)),
            pending_tx: Some(pending_rx),
            pending_tx_handle: Some(pending_tx_handle),
        }
    }

    /// Returns a channel to queue outbound NPDU frames from external callers.
    pub fn outbound_sender(&mut self) -> Option<mpsc::Sender<(u8, Bytes)>> {
        self.pending_tx_handle.clone()
    }

    /// Returns a shared counter for test assertions.
    pub fn token_received_count(&self) -> Arc<AtomicU64> {
        self.token_received_count.clone()
    }

    /// Drive the token-passing ring.  Consumes `self` and runs until the bus
    /// channel is closed.  Spawn this as a Tokio task.
    pub async fn run(mut self) {
        // Address 0 bootstraps the ring after a brief settle delay so other
        // masters have time to subscribe to the broadcast channel.
        if self.address == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
            let _ = self.bus_tx.send(MstpFrame {
                frame_type: MstpFrameType::Token,
                destination: 0,
                source: 0,
                data: Bytes::new(),
            });
            debug!(addr = 0, "MS/TP bootstrapped token ring");
        }

        // How long to wait for our token before declaring the ring broken.
        // Scale with ring size so larger rings don't time out prematurely.
        let ring_timeout = Duration::from_millis(TOKEN_WAIT_MS * (self.max_master as u64 + 2));

        let mut pending_rx = self.pending_tx.take().unwrap();

        loop {
            // ---- Wait for a frame addressed to us ----
            let got_token = timeout(ring_timeout, async {
                loop {
                    match self.bus_rx.recv().await {
                        Ok(frame) => {
                            if frame.frame_type == MstpFrameType::Token
                                && frame.destination == self.address
                            {
                                return true; // we have the token
                            }
                            // Respond to Poll-For-Master so the ring can discover us.
                            if frame.frame_type == MstpFrameType::PollForMaster
                                && frame.destination == self.address
                            {
                                let _ = self.bus_tx.send(MstpFrame {
                                    frame_type: MstpFrameType::ReplyToPollForMaster,
                                    destination: frame.source,
                                    source: self.address,
                                    data: Bytes::new(),
                                });
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            debug!(addr = self.address, skipped = n, "MS/TP lagged");
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            return false; // bus shut down
                        }
                    }
                }
            })
            .await;

            match got_token {
                Ok(true) => {
                    self.token_received_count.fetch_add(1, Ordering::Relaxed);
                    self.state = MasterState::UseToken;

                    // Drain pending outbound queue (send at most one frame per token).
                    if let Ok((dst, npdu)) = pending_rx.try_recv() {
                        let _ = self.bus_tx.send(MstpFrame {
                            frame_type: MstpFrameType::BacnetDataNotExpectingReply,
                            destination: dst,
                            source: self.address,
                            data: npdu,
                        });
                    }

                    // Pass token to next station.
                    self.pass_token();
                    self.state = MasterState::Idle;
                    // Brief yield so the recipient can process before we loop.
                    tokio::task::yield_now().await;
                }
                Ok(false) => {
                    // Bus closed — exit cleanly.
                    break;
                }
                Err(_) => {
                    // Ring timed out.  Address 0 restarts it.
                    if self.address == 0 {
                        debug!("MS/TP ring timeout, restarting");
                        let _ = self.bus_tx.send(MstpFrame {
                            frame_type: MstpFrameType::Token,
                            destination: 0,
                            source: 0,
                            data: Bytes::new(),
                        });
                    }
                }
            }
        }
    }

    /// Wait until this node receives the token (or `deadline` elapses).
    pub async fn wait_for_token(&mut self, deadline: Duration) -> Result<(), ()> {
        let result = timeout(deadline, async {
            loop {
                if let Ok(frame) = self.bus_rx.recv().await {
                    if frame.frame_type == MstpFrameType::Token && frame.destination == self.address
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mstp::virtual_link::VirtualMstpBus;
    use tokio::time::{sleep, Duration};

    /// All nodes in a 3-master ring should receive the token within 2 seconds.
    #[tokio::test]
    async fn token_ring_circulates_all_nodes() {
        let bus = VirtualMstpBus::new();
        let max_master = 2u8; // addresses 0, 1, 2

        let counters: Vec<_> = (0..=max_master)
            .map(|addr| {
                let master = VirtualMstpMaster::join(&bus, addr, max_master);
                let counter = master.token_received_count();
                tokio::spawn(master.run());
                counter
            })
            .collect();

        // Allow the ring to run for 1 second
        sleep(Duration::from_secs(1)).await;

        for (addr, counter) in counters.iter().enumerate() {
            let n = counter.load(Ordering::Relaxed);
            assert!(
                n > 0,
                "Master {addr} never received the token (got {n} times)"
            );
        }
    }

    /// A node that sends data via its outbound queue delivers the frame while
    /// holding the token.
    #[tokio::test]
    async fn data_frame_delivered_via_outbound_queue() {
        let bus = VirtualMstpBus::new();
        let max_master = 1u8;

        let mut master0 = VirtualMstpMaster::join(&bus, 0, max_master);
        let sender = master0.outbound_sender().unwrap();
        let master1 = VirtualMstpMaster::join(&bus, 1, max_master);

        // Snapshot master1's bus_rx before spawning, so we can observe frames.
        // We use a separate subscriber for the assertion.
        let mut obs_rx = bus.subscribe();

        tokio::spawn(master0.run());
        tokio::spawn(master1.run());

        // Queue a data frame from address 0 to address 1
        let payload = Bytes::from_static(b"\x01\x00\x00");
        sender
            .send((1, payload.clone()))
            .await
            .expect("send failed");

        // Wait up to 2 seconds for the data frame to appear on the bus
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        loop {
            match tokio::time::timeout_at(deadline, obs_rx.recv()).await {
                Ok(Ok(frame)) => {
                    if frame.frame_type == MstpFrameType::BacnetDataNotExpectingReply
                        && frame.destination == 1
                        && frame.data == payload
                    {
                        return; // success
                    }
                }
                Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => continue,
                _ => break,
            }
        }
        panic!("Data frame never appeared on the MS/TP bus");
    }
}
