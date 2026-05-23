/// Prometheus metrics exporter — Phase 6.
///
/// All metrics are registered once via `OnceLock` and updated from the
/// simulation engine tick loop and APDU dispatcher.
use prometheus::{
    exponential_buckets, register_counter_vec, register_gauge, register_histogram_vec, CounterVec,
    Encoder, Gauge, HistogramVec, TextEncoder,
};
use std::sync::OnceLock;

// ---------------------------------------------------------------------------
// Metric singletons
// ---------------------------------------------------------------------------

static DEVICES_TOTAL: OnceLock<Gauge> = OnceLock::new();
static OBJECTS_TOTAL: OnceLock<Gauge> = OnceLock::new();
static REQUESTS_TOTAL: OnceLock<CounterVec> = OnceLock::new();
static COV_NOTIFICATIONS_TOTAL: OnceLock<CounterVec> = OnceLock::new();
static ACTIVE_COV_SUBSCRIPTIONS: OnceLock<Gauge> = OnceLock::new();
static TICK_DURATION_SECONDS: OnceLock<HistogramVec> = OnceLock::new();

fn devices_total() -> &'static Gauge {
    DEVICES_TOTAL
        .get_or_init(|| register_gauge!("bacnet_devices_total", "Total simulated devices").unwrap())
}

fn objects_total() -> &'static Gauge {
    OBJECTS_TOTAL
        .get_or_init(|| register_gauge!("bacnet_objects_total", "Total simulated objects").unwrap())
}

pub fn requests_total() -> &'static CounterVec {
    REQUESTS_TOTAL.get_or_init(|| {
        register_counter_vec!(
            "bacnet_requests_total",
            "BACnet requests processed",
            &["service", "result"]
        )
        .unwrap()
    })
}

pub fn cov_notifications_total() -> &'static CounterVec {
    COV_NOTIFICATIONS_TOTAL.get_or_init(|| {
        register_counter_vec!(
            "bacnet_cov_notifications_total",
            "COV notifications sent",
            &["transport"]
        )
        .unwrap()
    })
}

pub fn active_cov_subscriptions() -> &'static Gauge {
    ACTIVE_COV_SUBSCRIPTIONS.get_or_init(|| {
        register_gauge!(
            "bacnet_active_cov_subscriptions",
            "Number of active COV subscriptions"
        )
        .unwrap()
    })
}

pub fn tick_duration_seconds() -> &'static HistogramVec {
    TICK_DURATION_SECONDS.get_or_init(|| {
        register_histogram_vec!(
            "bacnet_tick_duration_seconds",
            "Simulation tick duration in seconds",
            &["tick_hz"],
            exponential_buckets(0.001, 2.0, 12).unwrap()
        )
        .unwrap()
    })
}

// ---------------------------------------------------------------------------
// Convenience setters (called from the engine and dispatcher)
// ---------------------------------------------------------------------------

pub fn set_devices(n: f64) {
    devices_total().set(n);
}
pub fn set_objects(n: f64) {
    objects_total().set(n);
}
pub fn inc_requests(service: &str, result: &str) {
    requests_total().with_label_values(&[service, result]).inc();
}
pub fn inc_cov_notifications(transport: &str) {
    cov_notifications_total()
        .with_label_values(&[transport])
        .inc();
}
pub fn inc_cov_notifications_n(transport: &str, n: u64) {
    cov_notifications_total()
        .with_label_values(&[transport])
        .inc_by(n as f64);
}
pub fn set_active_cov_subscriptions(n: f64) {
    active_cov_subscriptions().set(n);
}
pub fn observe_tick_duration(tick_hz: f64, secs: f64) {
    tick_duration_seconds()
        .with_label_values(&[&format!("{tick_hz:.1}")])
        .observe(secs);
}

// ---------------------------------------------------------------------------
// Scrape endpoint
// ---------------------------------------------------------------------------

pub fn gather() -> String {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buf = Vec::new();
    encoder
        .encode(&metric_families, &mut buf)
        .unwrap_or_default();
    String::from_utf8(buf).unwrap_or_default()
}
