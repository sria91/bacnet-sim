/// BACnet/SC hub — accepts WebSocket connections from SC nodes, performs the
/// ConnectRequest/ConnectAccept handshake, and routes EncapsulatedNPDU frames
/// between connected nodes.
use std::net::SocketAddr;
use std::sync::Arc;

use bacnet_codec::sc::{ScFrame, ScFunction};
use bacnet_types::ScNodeId;
use bytes::Bytes;
use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, info, warn};

/// Per-node send handle kept in the hub registry.
struct NodeHandle {
    node_id: ScNodeId,
    #[allow(dead_code)]
    vmac: [u8; 6],
    tx: mpsc::Sender<Bytes>,
}

/// Central hub shared between the listener task and callers.
pub struct ScHub {
    /// Registry keyed by the node's virtual MAC address.
    nodes: Arc<DashMap<[u8; 6], NodeHandle>>,
    local_addr: SocketAddr,
}

impl ScHub {
    /// Start a hub listening on `addr`.  Returns `Arc<ScHub>` immediately;
    /// the accept loop runs in a spawned task.
    pub async fn start(addr: SocketAddr) -> std::io::Result<Arc<Self>> {
        let listener = TcpListener::bind(addr).await?;
        let local_addr = listener.local_addr()?;
        info!(%local_addr, "BACnet/SC hub listening");

        let hub = Arc::new(Self {
            nodes: Arc::new(DashMap::new()),
            local_addr,
        });

        let hub_clone = Arc::clone(&hub);
        tokio::spawn(async move {
            hub_clone.accept_loop(listener).await;
        });

        Ok(hub)
    }

    /// Address the hub is actually bound to (useful when port 0 was requested).
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Returns the node IDs of all currently connected nodes.
    pub async fn connected_nodes(&self) -> Vec<ScNodeId> {
        self.nodes.iter().map(|e| e.value().node_id).collect()
    }

    /// Send a raw SC frame payload to a node identified by `vmac`.
    pub async fn send_to_vmac(&self, vmac: [u8; 6], frame_bytes: Bytes) -> Result<(), ()> {
        if let Some(handle) = self.nodes.get(&vmac) {
            handle.tx.send(frame_bytes).await.map_err(|_| ())
        } else {
            Err(())
        }
    }

    // -----------------------------------------------------------------------
    // Internal accept loop
    // -----------------------------------------------------------------------

    async fn accept_loop(&self, listener: TcpListener) {
        loop {
            match listener.accept().await {
                Ok((stream, peer)) => {
                    debug!(%peer, "SC: new TCP connection");
                    let nodes = Arc::clone(&self.nodes);
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_connection(stream, peer, nodes).await {
                            warn!(%peer, error = %e, "SC: connection error");
                        }
                    });
                }
                Err(e) => {
                    warn!(error = %e, "SC hub accept error");
                    break;
                }
            }
        }
    }

    async fn handle_connection(
        stream: tokio::net::TcpStream,
        _peer: SocketAddr,
        nodes: Arc<DashMap<[u8; 6], NodeHandle>>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut ws = accept_async(stream).await?;

        // --- Handshake: expect ConnectRequest ---
        let (node_id, vmac) = loop {
            match ws.next().await {
                Some(Ok(Message::Binary(data))) => {
                    let frame = ScFrame::decode(&data)?;
                    if frame.function == ScFunction::ConnectRequest as u8 {
                        // Payload: node_id (16 bytes) + vmac (6 bytes)
                        if frame.payload.len() < 22 {
                            return Err("ConnectRequest payload too short".into());
                        }
                        let mut nid = [0u8; 16];
                        nid.copy_from_slice(&frame.payload[..16]);
                        let mut vm = [0u8; 6];
                        vm.copy_from_slice(&frame.payload[16..22]);
                        break (ScNodeId(nid), vm);
                    }
                }
                Some(Ok(Message::Close(_))) | None => {
                    return Err("Connection closed before handshake".into());
                }
                _ => {}
            }
        };

        // Send ConnectAccept
        let accept = ScFrame {
            function: ScFunction::ConnectAccept as u8,
            control: Default::default(),
            message_id: 0,
            originating_vmac: None,
            destination_vmac: None,
            payload: Bytes::new(),
        };
        ws.send(Message::Binary(accept.encode().to_vec().into()))
            .await?;
        info!(?node_id, vmac = ?vmac, "SC: node connected");

        // Register node
        let (tx, mut rx) = mpsc::channel::<Bytes>(64);
        nodes.insert(vmac, NodeHandle { node_id, vmac, tx });

        // Split the WS stream
        let (mut sink, mut stream) = ws.split();

        // Outbound task: forward queued frames to the WS
        let outbound = tokio::spawn(async move {
            while let Some(bytes) = rx.recv().await {
                if sink
                    .send(Message::Binary(bytes.to_vec().into()))
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });

        // Inbound loop: route EncapsulatedNPDU by destination vmac
        while let Some(msg) = stream.next().await {
            match msg {
                Ok(Message::Binary(data)) => {
                    if let Ok(frame) = ScFrame::decode(&data) {
                        if frame.function == ScFunction::EncapsulatedNpdu as u8 {
                            if let Some(dest_vmac) = frame.destination_vmac {
                                if let Some(dest) = nodes.get(&dest_vmac) {
                                    let _ = dest.tx.try_send(data);
                                }
                            }
                        } else if frame.function == ScFunction::Heartbeat as u8 {
                            // Reply with HeartbeatAck
                            let ack = ScFrame {
                                function: ScFunction::HeartbeatAck as u8,
                                control: Default::default(),
                                message_id: frame.message_id,
                                originating_vmac: None,
                                destination_vmac: None,
                                payload: Bytes::new(),
                            };
                            if let Some(handle) = nodes.get(&vmac) {
                                let _ = handle.tx.try_send(ack.encode());
                            }
                        }
                    }
                }
                Ok(Message::Close(_)) | Err(_) => break,
                _ => {}
            }
        }

        // Clean up
        nodes.remove(&vmac);
        outbound.abort();
        info!(?node_id, vmac = ?vmac, "SC: node disconnected");
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScHubError {
    NodeNotFound,
    SendFailed,
}

impl std::fmt::Display for ScHubError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NodeNotFound => write!(f, "SC node not found"),
            Self::SendFailed => write!(f, "SC send failed"),
        }
    }
}
