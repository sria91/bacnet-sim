/// APDU dispatcher — ties transport I/O to BACnet service handlers.
///
/// The dispatcher:
/// 1. Receives inbound NPDU frames from the transport layer.
/// 2. Decodes the NPDU header to extract the APDU.
/// 3. Identifies the PDU type and dispatches to the correct service handler.
/// 4. Encodes the response and sends it back via the transport's outbound channel.

use std::collections::HashMap;
use std::sync::Arc;

use bacnet_codec::{
    apdu::{
        ack::{ComplexAck, ComplexAckService, SimpleAck},
        confirmed::{ConfirmedRequest, ConfirmedServiceRequest},
        error::ErrorPdu,
        unconfirmed::UnconfirmedRequest,
    },
    npdu::Npdu,
};
use bacnet_object::store::ObjectStore;
use bacnet_types::{
    error::{BacnetError, ErrorClass, ErrorCode},
    DeviceId, NetworkAddress,
};
use bytes::BytesMut;
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, info, warn};

use bacnet_transport::{Destination, InboundFrame, OutboundFrame};

use crate::services::{read_property, read_property_multiple, write_property, who_is};

// ---------------------------------------------------------------------------
// Device registry
// ---------------------------------------------------------------------------

/// Static metadata for one simulated BACnet device.
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub device_id: DeviceId,
    pub max_apdu: u16,
    pub vendor_id: u16,
    pub vendor_name: String,
    pub model_name: String,
    pub firmware_revision: String,
    pub description: String,
}

impl DeviceInfo {
    pub fn new(device_id: u32) -> Self {
        Self {
            device_id: DeviceId(device_id),
            max_apdu: 1476,
            vendor_id: 999,
            vendor_name: "bacnet-sim".into(),
            model_name: "bacnet-sim".into(),
            firmware_revision: "1.0.0".into(),
            description: format!("Simulated device {device_id}"),
        }
    }
}

// ---------------------------------------------------------------------------
// Dispatcher
// ---------------------------------------------------------------------------

/// Routes inbound BACnet frames to service handlers and sends responses.
pub struct ApduDispatcher {
    devices: HashMap<u32, DeviceInfo>,
    store: Arc<ObjectStore>,
}

impl ApduDispatcher {
    pub fn new(store: Arc<ObjectStore>) -> Self {
        Self {
            devices: HashMap::new(),
            store,
        }
    }

    /// Register a device with the dispatcher.
    pub fn register_device(&mut self, info: DeviceInfo) {
        info!(device_id = info.device_id.0, "registered device");
        self.devices.insert(info.device_id.0, info);
    }

    /// Start the dispatcher receive loop.
    ///
    /// Consumes `self` and drives the loop until the inbound channel closes.
    pub async fn run(
        self,
        mut inbound: broadcast::Receiver<InboundFrame>,
        outbound: mpsc::Sender<OutboundFrame>,
    ) {
        let dispatcher = Arc::new(self);
        loop {
            match inbound.recv().await {
                Ok(frame) => {
                    let d = dispatcher.clone();
                    let tx = outbound.clone();
                    tokio::spawn(async move {
                        if let Err(e) = d.handle_frame(frame, tx).await {
                            debug!("dispatcher error: {e:?}");
                        }
                    });
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!("dispatcher lagged, dropped {n} frames");
                }
                Err(broadcast::error::RecvError::Closed) => {
                    info!("inbound channel closed, dispatcher exiting");
                    break;
                }
            }
        }
    }

    async fn handle_frame(
        &self,
        frame: InboundFrame,
        outbound: mpsc::Sender<OutboundFrame>,
    ) -> Result<(), BacnetError> {
        // Decode NPDU header to extract APDU bytes
        let npdu = Npdu::decode(&frame.npdu)?;
        if npdu.control.network_message {
            debug!("ignoring network message");
            return Ok(());
        }
        let apdu = &npdu.apdu;
        if apdu.is_empty() {
            return Ok(());
        }

        // PDU type from high nibble of first byte
        match apdu[0] & 0xF0 {
            0x00 => {
                // Confirmed-Request
                match ConfirmedRequest::decode(apdu) {
                    Ok(req) => {
                        self.handle_confirmed(req, frame.src, outbound).await?;
                    }
                    Err(e) => {
                        debug!("confirmed-request decode error: {e:?}");
                    }
                }
            }
            0x10 => {
                // Unconfirmed-Request
                match UnconfirmedRequest::decode(apdu) {
                    Ok(req) => {
                        self.handle_unconfirmed(req, frame.src, outbound).await?;
                    }
                    Err(e) => {
                        debug!("unconfirmed-request decode error: {e:?}");
                    }
                }
            }
            other => {
                debug!("ignoring APDU PDU type {other:#x}");
            }
        }

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Unconfirmed handlers
    // -----------------------------------------------------------------------

    async fn handle_unconfirmed(
        &self,
        req: UnconfirmedRequest,
        src: NetworkAddress,
        outbound: mpsc::Sender<OutboundFrame>,
    ) -> Result<(), BacnetError> {
        match req {
            UnconfirmedRequest::WhoIs(w) => {
                info!(low = ?w.low_limit, high = ?w.high_limit, from = ?src, "received Who-Is");
                // Each registered device that matches the range sends an I-Am
                for (_, dev) in &self.devices {
                    if let Some(iam) = who_is::handle_who_is(
                        w.low_limit,
                        w.high_limit,
                        dev.device_id,
                        dev.max_apdu,
                        dev.vendor_id,
                    ) {
                        let mut buf = BytesMut::new();
                        iam.encode(&mut buf);
                        let npdu_bytes =
                            Npdu::encode_local(false, &buf).to_vec();
                        // I-Am is broadcast
                        let _ = outbound
                            .send(OutboundFrame {
                                dst: Destination::Broadcast { network_number: 0 },
                                npdu: bytes::Bytes::from(npdu_bytes),
                            })
                            .await;
                        info!(device_id = dev.device_id.0, "I-Am sent");
                    }
                }
            }
            _ => {
                debug!("ignoring unconfirmed request");
            }
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Confirmed handlers
    // -----------------------------------------------------------------------

    async fn handle_confirmed(
        &self,
        req: ConfirmedRequest,
        src: NetworkAddress,
        outbound: mpsc::Sender<OutboundFrame>,
    ) -> Result<(), BacnetError> {
        // Find the target device.  In standard BACnet/IP each device has its
        // own IP address.  For the simulator we pick the single registered
        // device when there is only one, or the first otherwise.  Proper
        // multi-device routing is addressed in Phase 4.
        let dev = self
            .devices
            .values()
            .next()
            .ok_or(BacnetError::UnknownObject)?;

        let device_id = dev.device_id;
        let invoke_id = req.invoke_id;

        let result: Option<BytesMut> = match req.service {
            ConfirmedServiceRequest::ReadProperty(r) => {
                let service_choice = 12u8;
                match read_property::handle_read_property(r, &self.store, device_id).await {
                    Ok(complex_ack) => {
                        let mut buf = BytesMut::new();
                        complex_ack.encode(&mut buf);
                        Some(buf)
                    }
                    Err(e) => Some(encode_error_pdu(invoke_id, service_choice, &e)),
                }
            }

            ConfirmedServiceRequest::ReadPropertyMultiple(specs) => {
                let complex_ack =
                    read_property_multiple::handle_read_property_multiple(
                        specs, &self.store, device_id, invoke_id,
                    )
                    .await;
                let mut buf = BytesMut::new();
                complex_ack.encode(&mut buf);
                Some(buf)
            }

            ConfirmedServiceRequest::WriteProperty(w) => {
                let service_choice = 15u8;
                match write_property::handle_write_property(w, &self.store, device_id).await {
                    Ok(()) => {
                        let mut buf = BytesMut::new();
                        SimpleAck { invoke_id, service_choice }.encode(&mut buf);
                        Some(buf)
                    }
                    Err(e) => Some(encode_error_pdu(invoke_id, service_choice, &e)),
                }
            }

            ConfirmedServiceRequest::SubscribeCov(_s) => {
                // COV subscriptions: Phase 3 — for now send SimpleACK
                let service_choice = 5u8;
                let mut buf = BytesMut::new();
                SimpleAck { invoke_id, service_choice }.encode(&mut buf);
                Some(buf)
            }
        };

        if let Some(apdu_bytes) = result {
            let npdu = Npdu::encode_local(false, &apdu_bytes);
            let _ = outbound
                .send(OutboundFrame {
                    dst: Destination::Unicast(src),
                    npdu,
                })
                .await;
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Error PDU encoding
// ---------------------------------------------------------------------------

fn encode_error_pdu(invoke_id: u8, service_choice: u8, e: &BacnetError) -> BytesMut {
    let (class, code) = match e {
        BacnetError::UnknownObject => (ErrorClass::Object, ErrorCode::UnknownObject),
        BacnetError::UnknownProperty => (ErrorClass::Property, ErrorCode::UnknownProperty),
        BacnetError::WriteAccessDenied => {
            (ErrorClass::Property, ErrorCode::WriteAccessDenied)
        }
        BacnetError::ValueOutOfRange => (ErrorClass::Property, ErrorCode::ValueOutOfRange),
        BacnetError::InvalidDataType => (ErrorClass::Property, ErrorCode::InvalidDataType),
        BacnetError::ServiceError { error_class, error_code } => {
            (*error_class, *error_code)
        }
        _ => (ErrorClass::Services, ErrorCode::Other(0)),
    };
    let mut buf = BytesMut::new();
    ErrorPdu {
        invoke_id,
        service: service_choice,
        error_class: class,
        error_code: code,
    }
    .encode(&mut buf);
    buf
}

fn u8_to_service_choice(
    _v: u8,
) -> bacnet_codec::apdu::confirmed::ConfirmedServiceChoice {
    bacnet_codec::apdu::confirmed::ConfirmedServiceChoice::ReadProperty
}

