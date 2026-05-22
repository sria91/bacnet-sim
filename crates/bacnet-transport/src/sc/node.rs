/// BACnet/SC node — client-side WebSocket connection to a hub.
///
/// Performs the ConnectRequest/ConnectAccept handshake, then runs a bidirectional
/// message loop.  Reconnects automatically after disconnection.

use std::time::Duration;

use bacnet_codec::sc::{ScFrame, ScFunction};
use bacnet_types::ScNodeId;
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, info, warn};

use super::ScConnectionState;

/// A handle to a running SC node.  The node loop runs in a spawned task; use
/// `ScNodeHandle` to send/receive NPDUs.
pub struct ScNodeHandle {
    pub node_id: ScNodeId,
    pub vmac: [u8; 6],
    /// Send raw SC-frame bytes outbound to the hub.
    pub tx: mpsc::Sender<Bytes>,
    /// Receive raw SC-frame bytes inbound from the hub.
    pub rx: mpsc::Receiver<Bytes>,
    state: ScConnectionState,
}

impl ScNodeHandle {
    pub fn connection_state(&self) -> ScConnectionState {
        self.state
    }

    pub fn vmac(&self) -> [u8; 6] {
        self.vmac
    }

    /// Send an EncapsulatedNPDU frame to the given destination vmac through the hub.
    pub async fn send_npdu(
        &self,
        dest_vmac: [u8; 6],
        npdu: Bytes,
        message_id: u16,
    ) -> Result<(), ()> {
        let frame = ScFrame::encapsulated_npdu(message_id, Some(self.vmac), Some(dest_vmac), npdu);
        self.tx.send(frame.encode()).await.map_err(|_| ())
    }

    /// Receive the next inbound SC frame (blocks until available).
    pub async fn recv_frame(&mut self) -> Option<ScFrame> {
        let bytes = self.rx.recv().await?;
        ScFrame::decode(&bytes).ok()
    }
}

/// Connect to a BACnet/SC hub at `url` (e.g. `"ws://127.0.0.1:47814"`).
///
/// Performs the ConnectRequest handshake and returns a `ScNodeHandle` on
/// success, then spawns the background read/write loop.
pub async fn connect(
    url: &str,
    node_id: ScNodeId,
    vmac: [u8; 6],
) -> Result<ScNodeHandle, Box<dyn std::error::Error + Send + Sync>> {
    let (ws, _) = connect_async(url).await?;
    let (mut sink, mut stream) = ws.split();

    // Send ConnectRequest: payload = node_id (16B) + vmac (6B)
    let mut payload = Vec::with_capacity(22);
    payload.extend_from_slice(&node_id.0);
    payload.extend_from_slice(&vmac);
    let req = ScFrame {
        function: ScFunction::ConnectRequest as u8,
        control: Default::default(),
        message_id: 1,
        originating_vmac: None, // vmac is carried in payload, not frame header
        destination_vmac: None,
        payload: Bytes::from(payload),
    };
    sink.send(Message::Binary(req.encode().to_vec().into())).await?;

    // Wait for ConnectAccept
    loop {
        match stream.next().await {
            Some(Ok(Message::Binary(data))) => {
                let frame = ScFrame::decode(&data)?;
                if frame.function == ScFunction::ConnectAccept as u8 {
                    info!(?node_id, vmac = ?vmac, "SC: connected to hub");
                    break;
                }
            }
            Some(Ok(Message::Close(_))) | None => {
                return Err("Hub closed connection before ConnectAccept".into());
            }
            _ => {}
        }
    }

    // Channels for the application layer
    let (out_tx, out_rx) = mpsc::channel::<Bytes>(64); // app → node loop → hub
    let (in_tx, in_rx) = mpsc::channel::<Bytes>(64);   // hub → node loop → app

    // Spawn the I/O loop
    tokio::spawn(async move {
        run_node_loop(sink, stream, out_rx, in_tx).await;
    });

    Ok(ScNodeHandle {
        node_id,
        vmac,
        tx: out_tx,
        rx: in_rx,
        state: ScConnectionState::Connected,
    })
}

/// Reconnect loop: retries with exponential back-off.
pub async fn connect_with_retry(
    url: String,
    node_id: ScNodeId,
    vmac: [u8; 6],
) -> ScNodeHandle {
    let mut delay = Duration::from_millis(100);
    loop {
        match connect(&url, node_id, vmac).await {
            Ok(handle) => return handle,
            Err(e) => {
                warn!(error = %e, ?delay, "SC: connect failed, retrying");
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_secs(30));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Internal I/O loop
// ---------------------------------------------------------------------------

use futures_util::stream::SplitSink;
use futures_util::stream::SplitStream;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
type WsSink = SplitSink<WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>, Message>;
type WsStream = SplitStream<WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>>;

async fn run_node_loop(
    mut sink: WsSink,
    mut stream: WsStream,
    mut out_rx: mpsc::Receiver<Bytes>,
    in_tx: mpsc::Sender<Bytes>,
) {
    // Forward queued outbound frames to the WS sink
    let write_loop = tokio::spawn(async move {
        while let Some(bytes) = out_rx.recv().await {
            if sink.send(Message::Binary(bytes.to_vec().into())).await.is_err() {
                break;
            }
        }
    });

    while let Some(msg) = stream.next().await {
        match msg {
            Ok(Message::Binary(data)) => {
                let bytes = Bytes::from(data.to_vec());
                if let Ok(frame) = ScFrame::decode(&bytes) {
                    if frame.function == ScFunction::Heartbeat as u8 {
                        // Heartbeats are handled at transport level; drop silently
                        debug!("SC node: received Heartbeat (ignored in loop)");
                    } else {
                        let _ = in_tx.send(bytes).await;
                    }
                }
            }
            Ok(Message::Close(_)) | Err(_) => {
                debug!("SC node WS closed");
                break;
            }
            _ => {}
        }
    }

    write_loop.abort();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sc::hub::ScHub;

    /// A node can connect to the hub and the hub registers it.
    #[tokio::test]
    async fn node_connects_and_hub_registers_it() {
        let hub = ScHub::start("127.0.0.1:0".parse().unwrap())
            .await
            .expect("hub start failed");

        let addr = hub.local_addr();
        let url = format!("ws://{addr}");
        let node_id = ScNodeId::random();
        let vmac = [0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x01];

        let _handle = connect(&url, node_id, vmac)
            .await
            .expect("connect failed");

        // Allow the hub a moment to register the node
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let nodes = hub.connected_nodes().await;
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0], node_id);
    }

    /// Two nodes can communicate via the hub by routing EncapsulatedNPDU frames.
    #[tokio::test]
    async fn npdu_routes_between_two_nodes() {
        let hub = ScHub::start("127.0.0.1:0".parse().unwrap())
            .await
            .expect("hub start failed");

        let addr = hub.local_addr();
        let url = format!("ws://{addr}");

        let node_id_a = ScNodeId::random();
        let vmac_a = [0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0x01];
        let mut handle_a = connect(&url, node_id_a, vmac_a)
            .await
            .expect("node A connect failed");

        let node_id_b = ScNodeId::random();
        let vmac_b = [0xBB, 0xBB, 0xBB, 0xBB, 0xBB, 0x02];
        let mut handle_b = connect(&url, node_id_b, vmac_b)
            .await
            .expect("node B connect failed");

        // Small settle time for hub to register both nodes
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Node A sends an NPDU to Node B
        let test_npdu = Bytes::from_static(b"\x01\x00\x00\xFF");
        handle_a
            .send_npdu(vmac_b, test_npdu.clone(), 42)
            .await
            .expect("send_npdu failed");

        // Node B should receive an EncapsulatedNPDU frame within 1 second
        let received = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            handle_b.recv_frame(),
        )
        .await
        .expect("timeout waiting for frame")
        .expect("channel closed");

        assert_eq!(received.function, ScFunction::EncapsulatedNpdu as u8);
        assert_eq!(received.destination_vmac, Some(vmac_b));
        assert_eq!(received.payload, test_npdu);
    }
}

