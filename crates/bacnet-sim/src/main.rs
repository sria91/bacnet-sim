mod simulation_builder;
mod hot_reload;

use std::{net::SocketAddr, sync::Arc};
use tracing::info;

use bacnet_object::{
    analog_input::AnalogInput,
    binary_input::BinaryInput,
    device::DeviceObject,
    store::ObjectStore,
};
use bacnet_sim_engine::engine::SimEngine;
use bacnet_stack::dispatcher::{ApduDispatcher, DeviceInfo};
use bacnet_transport::ip::BacnetIpTransport;
use bacnet_transport::sc::hub::ScHub;
use bacnet_config::topology::TransportKind;
use bacnet_types::{
    property_value::EngineeringUnits,
    DeviceId,
};
use bacnet_api::rest::AppState;

const DEVICE_ID: u32 = 1234;
const BACNET_PORT: u16 = 47808;
const API_PORT: u16 = 8080;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // -----------------------------------------------------------------------
    // Structured logging — default to INFO; --log-format=json for JSON output
    // -----------------------------------------------------------------------
    let log_format = std::env::args()
        .find(|a| a.starts_with("--log-format="))
        .map(|a| a.trim_start_matches("--log-format=").to_string())
        .unwrap_or_else(|| "text".to_string());

    match log_format.as_str() {
        "json" => {
            tracing_subscriber::fmt()
                .json()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::from_default_env()
                        .add_directive("info".parse()?),
                )
                .init();
        }
        _ => {
            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::from_default_env()
                        .add_directive("info".parse()?),
                )
                .init();
        }
    }

    info!(version = env!("CARGO_PKG_VERSION"), "bacnet-sim starting");

    // -----------------------------------------------------------------------
    // Parse optional --config <path> argument
    // -----------------------------------------------------------------------
    let config_path = parse_config_arg();

    // -----------------------------------------------------------------------
    // Object store + API state
    // -----------------------------------------------------------------------
    let store = Arc::new(ObjectStore::new());
    let api_state = AppState::new(Arc::clone(&store));

    let bind_addr: SocketAddr = format!("0.0.0.0:{BACNET_PORT}").parse()?;
    let transport = BacnetIpTransport::bind(bind_addr).await?;
    let inbound_rx = transport.subscribe();
    let outbound_tx = transport.sender();

    let mut dispatcher = ApduDispatcher::new(Arc::clone(&store));

    let sim_engine: SimEngine;

    if let Some(ref path) = config_path {
        // ----- Config-driven mode -----
        let toml_str = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("Failed to read config file {path:?}: {e}"))?;
        let config = bacnet_config::topology::SimulatorConfig::from_toml(&toml_str)
            .map_err(|e| anyhow::anyhow!("Failed to parse config: {e}"))?;
        info!(path = %path, "Loading topology from config file");
        // Start transport(s) specified in the config
        for net in &config.networks {
            match net.transport {
                TransportKind::BacnetSc => {
                    let sc_bind: SocketAddr = net
                        .bind
                        .as_deref()
                        .unwrap_or("0.0.0.0:47814")
                        .parse()
                        .unwrap_or_else(|_| "0.0.0.0:47814".parse().unwrap());
                    match ScHub::start(sc_bind).await {
                        Ok(hub) => info!(addr = %hub.local_addr(), "BACnet/SC hub listening"),
                        Err(e) => tracing::error!("Failed to start SC hub: {e}"),
                    }
                }
                TransportKind::BacnetIp | TransportKind::Mstp => {
                    // BACnet/IP is started above; MS/TP virtual bus not yet wired here
                }
            }
        }
        // Register all device IDs so the REST API can enumerate them
        for group in &config.devices {
            for id in group.id_range[0]..=group.id_range[1] {
                api_state.register_device(id).await;
                dispatcher.register_device(DeviceInfo::new(id));
            }
        }
        sim_engine =
            simulation_builder::build_simulation(&config, Arc::clone(&store), &mut dispatcher);
    } else {
        // ----- Demo mode: single device with 8 AI + 4 BI -----
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

        dispatcher.register_device(DeviceInfo::new(DEVICE_ID));
        api_state.register_device(DEVICE_ID).await;

        let (engine, cov_rx) = SimEngine::new(Arc::clone(&store), 1.0);
        tokio::spawn(async move {
            let mut rx = cov_rx;
            while let Some(notif) = rx.recv().await {
                tracing::debug!(
                    device = notif.device_id.0,
                    object = ?notif.object_id,
                    "COV notification"
                );
            }
        });
        sim_engine = engine;

        info!(device_id = DEVICE_ID, "Demo mode: single device, 12 objects");
    }

    // -----------------------------------------------------------------------
    // Hot-reload watcher (config-driven mode only)
    // -----------------------------------------------------------------------
    if let Some(path) = config_path.clone() {
        let store_ref = Arc::clone(&store);
        tokio::spawn(async move {
            hot_reload::watch(path, store_ref).await;
        });
    }

    bacnet_api::metrics::set_objects(store.count() as f64);

    info!(
        total_objects = store.count(),
        "Object store ready",
    );

    // -----------------------------------------------------------------------
    // Background metrics refresh (every 15s)
    // -----------------------------------------------------------------------
    let tick_nanos = Arc::clone(&sim_engine.last_tick_nanos);
    let cov_total = Arc::clone(&sim_engine.cov_notifications_total);
    let tick_hz = sim_engine.tick_hz;
    let store_m = Arc::clone(&store);
    tokio::spawn(async move {
        use std::sync::atomic::Ordering;
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(15));
        loop {
            interval.tick().await;
            let nanos = tick_nanos.load(Ordering::Relaxed);
            if nanos > 0 {
                bacnet_api::metrics::observe_tick_duration(tick_hz, nanos as f64 / 1e9);
            }
            let cov = cov_total.load(Ordering::Relaxed);
            bacnet_api::metrics::set_active_cov_subscriptions(cov as f64);
            bacnet_api::metrics::set_objects(store_m.count() as f64);
        }
    });

    // -----------------------------------------------------------------------
    // APDU dispatcher
    // -----------------------------------------------------------------------
    let dispatch_task = tokio::spawn(async move {
        dispatcher.run(inbound_rx, outbound_tx).await;
    });

    // -----------------------------------------------------------------------
    // Simulation engine
    // -----------------------------------------------------------------------
    let _engine_task = tokio::spawn(sim_engine.run());

    // -----------------------------------------------------------------------
    // Management REST API
    // -----------------------------------------------------------------------
    let api_addr: SocketAddr = format!("0.0.0.0:{API_PORT}").parse()?;
    let _api_task = tokio::spawn(async move {
        if let Err(e) = bacnet_api::rest::serve(api_addr, api_state).await {
            tracing::error!("REST API error: {e}");
        }
    });

    info!("BACnet/IP listening on udp/0.0.0.0:{BACNET_PORT}");
    info!("Management API listening on http://0.0.0.0:{API_PORT}");

    // -----------------------------------------------------------------------
    // Run transport or Ctrl-C
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

/// Extract `--config <path>` from process arguments, if present.
fn parse_config_arg() -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--config" {
            return args.next();
        }
        if let Some(path) = arg.strip_prefix("--config=") {
            return Some(path.to_string());
        }
    }
    None
}
