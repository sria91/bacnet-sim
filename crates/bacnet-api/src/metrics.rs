/// Prometheus metrics exporter.

use prometheus::{
    register_gauge, register_counter_vec, register_histogram,
    Gauge, CounterVec, Histogram, TextEncoder, Encoder,
};
use std::sync::OnceLock;

static DEVICES_TOTAL: OnceLock<Gauge> = OnceLock::new();
static OBJECTS_TOTAL: OnceLock<Gauge> = OnceLock::new();
static REQUESTS_TOTAL: OnceLock<CounterVec> = OnceLock::new();
static TICK_DURATION: OnceLock<Histogram> = OnceLock::new();

fn devices_total() -> &'static Gauge {
    DEVICES_TOTAL.get_or_init(|| {
        register_gauge!("bacnet_devices_total", "Total simulated devices").unwrap()
    })
}

fn objects_total() -> &'static Gauge {
    OBJECTS_TOTAL.get_or_init(|| {
        register_gauge!("bacnet_objects_total", "Total simulated objects").unwrap()
    })
}

pub fn requests_total() -> &'static CounterVec {
    REQUESTS_TOTAL.get_or_init(|| {
        register_counter_vec!(
            "bacnet_requests_total",
            "BACnet requests processed",
            &["service", "result"]
        ).unwrap()
    })
}

pub fn set_devices(n: f64) { devices_total().set(n); }
pub fn set_objects(n: f64) { objects_total().set(n); }

pub fn gather() -> String {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buf = Vec::new();
    encoder.encode(&metric_families, &mut buf).unwrap_or_default();
    String::from_utf8(buf).unwrap_or_default()
}
