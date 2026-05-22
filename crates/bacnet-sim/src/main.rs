use std::{net::SocketAddr, sync::Arc};
use tracing::info;
use tracing_subscriber::EnvFilter;

use bacnet_object::{
    analog_input::AnalogInput,
    binary_input::BinaryInput,
    device::DeviceObject,
    store::ObjectStore,
};
use bacnet_sim_engine::engine::SimEngine;
use bacnet_stack::dispatcher::{ApduDispatcher, DeviceInfo};
use bacnet_transport::ip::BacnetIpTransport;
use bacnet_types::{
    property_value::EngineeringUnits,
    DeviceId,
};

const DEVICE_ID: u32 = 1234;
const BACNET_PORT: u16 = 47808;
const API_PORT: u16 = 8080;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Structured logging — default to INFO, override with RUST_LOG
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();

    info!(version = env!("CARGO_PKG_VERSION"), "bacnet-sim starting");

    // -----------------------------------------------------------------------
    // Object store — create device + a handful of demo objects
    // -----------------------------------------------------------------------
    let store = Arc::new(ObjectStore::new());
    let device_id = DeviceId(DEVICE_ID);

    let device_obj = DeviceObject::new(device_id, "bacnet-sim-device");
    store.insert(device_id, Box::new(device_obj));

    for i in 1..=8u32 {
        let ai = AnalogInput::new(
            device_id,
            i,
            format!("AI-{i:02}"),
            EngineeringUnits::DegreesCelsius,
        );
        store.insert(device_id, Box::new(ai));
    }
    for i in 1..=4u32 {
        let bi = BinaryInput::new(device_id, i, format!("BI-{i:02}"));
        store.insert(device_id, Box::new(bi));
    }

    info!(
        device_id = DEVICE_ID,
        "Object store initialised",
    );

    // -----------------------------------------------------------------------
    // BACnet/IP transport
    // -----------------------------------------------------------------------
    let bind_addr: SocketAddr = format!("0.0.0.0:{BACNET_PORT}").parse()?;
    let transport = BacnetIpTransport::bind(bind_addr).await?;
    let inbound_rx = transport.subscribe();
    let outbound_tx = transport.sender();

    // -----------------------------------------------------------------------
    // APDU dispatcher
    // -----------------------------------------------------------------------
    let mut dispatcher = ApduDispatcher::new(Arc::clone(&store));
    dispatcher.register_device(DeviceInfo::new(DEVICE_ID));

    let dispatch_task = tokio::spawn(async move {
        dispatcher.run(inbound_rx, outbound_tx).await;
    });

    // -----------------------------------------------------------------------
    // Simulation engine — ticks value models at 1 Hz
    // -----------------------------------------------------------------------
    let (sim_engine, cov_rx) = SimEngine::new(Arc::clone(&store), 1.0);
    let _cov_drain = tokio::spawn(async move {
        let mut cov_rx = cov_rx;
        while let Some(notif) = cov_rx.recv().await {
            tracing::debug!(
                device = notif.device_id.0,
                object = ?notif.object_id,
                props = notif.changed_properties.len(),
                "COV notification"
            );
        }
    });
    let _engine_task = tokio::spawn(sim_engine.run());

    // -----------------------------------------------------------------------
    // Management REST API
    // -----------------------------------------------------------------------
    let api_addr: SocketAddr = format!("0.0.0.0:{API_PORT}").parse()?;
    let _api_task = tokio::spawn(async move {
        if let Err(e) = bacnet_api::rest::serve(api_addr).await {
            tracing::error!("REST API error: {e}");
        }
    });

    info!("BACnet/IP listening on udp/0.0.0.0:{BACNET_PORT}");
    info!("Management API listening on http://0.0.0.0:{API_PORT}");

    // -----------------------------------------------------------------------
    // Run transport (blocks until error) or Ctrl-C
    // -----------------------------------------------------------------------
    tokio::select! {
        _ = transport.run() => {
            tracing::error!("Transport exited unexpectedly");
        }
        _ = tokio::signal::ctrl_c() => {
            info!("Ctrl-C received, shutting down");
        }
    }

    dispatch_task.abort();
    Ok(())
}

