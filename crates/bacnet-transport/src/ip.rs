/// BACnet/IP UDP transport (BVLL layer).

use bacnet_codec::bvll::BvllFrame;
use bacnet_types::NetworkAddress;
use bytes::Bytes;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, error, warn};

use crate::{Destination, InboundFrame, OutboundFrame};

pub const BACNET_IP_PORT: u16 = 47808;

pub struct BacnetIpTransport {
    socket: Arc<UdpSocket>,
    inbound_tx: broadcast::Sender<InboundFrame>,
    outbound_rx: mpsc::Receiver<OutboundFrame>,
    outbound_tx: mpsc::Sender<OutboundFrame>,
}

impl BacnetIpTransport {
    pub async fn bind(addr: SocketAddr) -> std::io::Result<Self> {
        let socket = UdpSocket::bind(addr).await?;
        socket.set_broadcast(true)?;
        let socket = Arc::new(socket);
        let (inbound_tx, _) = broadcast::channel(1024);
        let (outbound_tx, outbound_rx) = mpsc::channel(1024);
        Ok(Self { socket, inbound_tx, outbound_rx, outbound_tx })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<InboundFrame> {
        self.inbound_tx.subscribe()
    }

    pub fn sender(&self) -> mpsc::Sender<OutboundFrame> {
        self.outbound_tx.clone()
    }

    pub async fn run(mut self) {
        let socket_recv = self.socket.clone();
        let socket_send = self.socket.clone();
        let inbound_tx = self.inbound_tx.clone();

        // Receive loop
        tokio::spawn(async move {
            let mut buf = vec![0u8; 65536];
            loop {
                match socket_recv.recv_from(&mut buf).await {
                    Ok((n, src)) => {
                        match BvllFrame::decode(&buf[..n]) {
                            Ok(frame) => {
                                let npdu = match frame {
                                    BvllFrame::OriginalUnicastNpdu(d)
                                    | BvllFrame::OriginalBroadcastNpdu(d)
                                    | BvllFrame::DistributeBroadcastToNetwork(d) => d,
                                    BvllFrame::ForwardedNpdu { npdu, .. } => npdu,
                                    _ => {
                                        debug!("unhandled BVLL function from {src}");
                                        continue;
                                    }
                                };
                                let src_mac = match src {
                                    SocketAddr::V4(v4) => bacnet_types::MacAddr::Ip(v4),
                                    _ => continue,
                                };
                                let src_addr = NetworkAddress {
                                    network_number: 0,
                                    mac: src_mac,
                                };
                                let _ = inbound_tx.send(InboundFrame { src: src_addr, npdu });
                            }
                            Err(e) => warn!("BVLL decode error from {src}: {e}"),
                        }
                    }
                    Err(e) => {
                        error!("UDP recv error: {e}");
                        break;
                    }
                }
            }
        });

        // Send loop
        while let Some(frame) = self.outbound_rx.recv().await {
            let bvll = match &frame.dst {
                Destination::Unicast(_) => BvllFrame::OriginalUnicastNpdu(frame.npdu),
                Destination::Broadcast { .. } => BvllFrame::OriginalBroadcastNpdu(frame.npdu),
            };
            let encoded = bvll.encode();
            let dst_addr: SocketAddr = match &frame.dst {
                Destination::Unicast(addr) => match addr.mac {
                    bacnet_types::MacAddr::Ip(v4) => SocketAddr::V4(v4),
                    _ => continue,
                },
                Destination::Broadcast { .. } => {
                    "255.255.255.255:47808".parse().unwrap()
                }
            };
            if let Err(e) = socket_send.send_to(&encoded, dst_addr).await {
                error!("UDP send error to {dst_addr}: {e}");
            }
        }
    }
}
