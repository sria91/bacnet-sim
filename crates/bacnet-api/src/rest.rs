/// Axum HTTP management API — Phase 6.
///
/// Routes:
///   GET  /api/v1/health
///   GET  /api/v1/metrics          — Prometheus text format
///   GET  /api/v1/devices          — list registered devices
///   GET  /api/v1/devices/:id      — device detail
///   GET  /api/v1/devices/:id/objects             — list objects on a device
///   GET  /api/v1/devices/:id/objects/:type/:inst — object detail
///   PUT  /api/v1/devices/:id/objects/:type/:inst/properties/:prop — write property
///   POST /api/v1/scenarios/load   — load a scenario TOML from a path
///   POST /api/v1/scenarios/stop   — no-op stub (placeholder)
use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::info;

use bacnet_object::store::ObjectStore;
use bacnet_types::{DeviceId, ObjectId, ObjectType};

// ---------------------------------------------------------------------------
// Shared application state
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct AppState {
    pub store: Arc<ObjectStore>,
    /// Registered device IDs (in insertion order).
    pub device_ids: Arc<tokio::sync::RwLock<Vec<u32>>>,
}

impl AppState {
    pub fn new(store: Arc<ObjectStore>) -> Self {
        Self {
            store,
            device_ids: Arc::new(tokio::sync::RwLock::new(Vec::new())),
        }
    }

    pub async fn register_device(&self, id: u32) {
        let mut ids = self.device_ids.write().await;
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    version: &'static str,
    objects_total: usize,
    devices_total: usize,
}

#[derive(Serialize)]
struct DeviceListItem {
    id: u32,
    object_count: usize,
}

#[derive(Serialize)]
struct ObjectListItem {
    object_type: String,
    instance: u32,
}

#[derive(Serialize)]
struct ObjectDetail {
    object_type: String,
    instance: u32,
    properties: HashMap<String, serde_json::Value>,
}

#[derive(Deserialize)]
struct WritePropertyBody {
    value: serde_json::Value,
}

#[derive(Deserialize)]
struct LoadScenarioBody {
    path: String,
}

// ---------------------------------------------------------------------------
// Error helper
// ---------------------------------------------------------------------------

struct ApiError(StatusCode, String);
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, self.1).into_response()
    }
}
fn not_found(msg: impl Into<String>) -> ApiError {
    ApiError(StatusCode::NOT_FOUND, msg.into())
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let devices = state.device_ids.read().await.len();
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        objects_total: state.store.count(),
        devices_total: devices,
    })
}

async fn metrics_handler() -> String {
    crate::metrics::gather()
}

async fn list_devices(State(state): State<AppState>) -> impl IntoResponse {
    let ids = state.device_ids.read().await.clone();
    let items: Vec<DeviceListItem> = ids
        .iter()
        .map(|&id| {
            let dev = DeviceId(id);
            let count = count_objects_for_device(&state.store, dev);
            DeviceListItem {
                id,
                object_count: count,
            }
        })
        .collect();
    Json(items)
}

async fn get_device(
    State(state): State<AppState>,
    Path(id): Path<u32>,
) -> Result<impl IntoResponse, ApiError> {
    let ids = state.device_ids.read().await;
    if !ids.contains(&id) {
        return Err(not_found(format!("Device {id} not found")));
    }
    let dev = DeviceId(id);
    Ok(Json(DeviceListItem {
        id,
        object_count: count_objects_for_device(&state.store, dev),
    }))
}

async fn list_objects(
    State(state): State<AppState>,
    Path(id): Path<u32>,
) -> Result<impl IntoResponse, ApiError> {
    {
        let ids = state.device_ids.read().await;
        if !ids.contains(&id) {
            return Err(not_found(format!("Device {id} not found")));
        }
    }
    let dev = DeviceId(id);
    let items = collect_objects(&state.store, dev);
    Ok(Json(items))
}

async fn get_object(
    State(state): State<AppState>,
    Path((dev_id, obj_type, inst)): Path<(u32, String, u32)>,
) -> Result<impl IntoResponse, ApiError> {
    let otype = parse_object_type(&obj_type)?;
    let dev = DeviceId(dev_id);
    let oid = ObjectId {
        object_type: otype,
        instance: inst,
    };
    let obj_ref = state.store.get(dev, oid).ok_or_else(|| {
        not_found(format!(
            "Object {obj_type}/{inst} on device {dev_id} not found"
        ))
    })?;
    let guard = obj_ref.read_guard();
    let properties = guard
        .all_properties()
        .into_iter()
        .map(|(k, v)| (format!("{k:?}"), property_to_json(v)))
        .collect();
    Ok(Json(ObjectDetail {
        object_type: obj_type,
        instance: inst,
        properties,
    }))
}

async fn write_property(
    State(state): State<AppState>,
    Path((dev_id, obj_type, inst, prop)): Path<(u32, String, u32, String)>,
    Json(body): Json<WritePropertyBody>,
) -> Result<impl IntoResponse, ApiError> {
    let otype = parse_object_type(&obj_type)?;
    let dev = DeviceId(dev_id);
    let oid = ObjectId {
        object_type: otype,
        instance: inst,
    };
    let pid = parse_property_id(&prop)?;

    let mut obj_ref = state
        .store
        .get(dev, oid)
        .ok_or_else(|| not_found("Object not found".to_string()))?;

    let pv = json_to_property_value(&body.value, pid)
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, e))?;

    obj_ref
        .write_guard()
        .force_write_property(pid, pv)
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, format!("{e:?}")))?;

    Ok(StatusCode::NO_CONTENT)
}

async fn load_scenario(Json(body): Json<LoadScenarioBody>) -> Result<impl IntoResponse, ApiError> {
    // Verify the path exists and is readable (actual scenario application is a
    // future enhancement — for now we validate and acknowledge).
    std::fs::metadata(&body.path).map_err(|e| {
        ApiError(
            StatusCode::BAD_REQUEST,
            format!("Cannot access {}: {e}", body.path),
        )
    })?;
    Ok(Json(
        serde_json::json!({ "status": "accepted", "path": body.path }),
    ))
}

async fn stop_scenario() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "stopped" }))
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

pub async fn serve(addr: SocketAddr, state: AppState) -> std::io::Result<()> {
    let app = Router::new()
        .route("/api/v1/health", get(health))
        .route("/api/v1/metrics", get(metrics_handler))
        .route("/api/v1/devices", get(list_devices))
        .route("/api/v1/devices/{id}", get(get_device))
        .route("/api/v1/devices/{id}/objects", get(list_objects))
        .route(
            "/api/v1/devices/{id}/objects/{type}/{inst}",
            get(get_object),
        )
        .route(
            "/api/v1/devices/{id}/objects/{type}/{inst}/properties/{prop}",
            put(write_property),
        )
        .route("/api/v1/scenarios/load", post(load_scenario))
        .route("/api/v1/scenarios/stop", post(stop_scenario))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    info!("Management API listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn count_objects_for_device(store: &ObjectStore, dev: DeviceId) -> usize {
    use bacnet_types::property_id::PropertyIdentifier;
    let oid = ObjectId {
        object_type: ObjectType::Device,
        instance: dev.0,
    };
    if let Some(obj) = store.get(dev, oid) {
        // Try to read ObjectList length
        if let Ok(bacnet_types::property_value::PropertyValue::Unsigned(n)) = obj
            .read_guard()
            .read_property(PropertyIdentifier::ObjectList, Some(0))
        {
            return n as usize;
        }
    }
    0
}

fn collect_objects(store: &ObjectStore, dev: DeviceId) -> Vec<ObjectListItem> {
    use bacnet_types::property_id::PropertyIdentifier;
    let oid = ObjectId {
        object_type: ObjectType::Device,
        instance: dev.0,
    };
    if let Some(obj) = store.get(dev, oid) {
        let guard = obj.read_guard();
        if let Ok(bacnet_types::property_value::PropertyValue::Array(list)) =
            guard.read_property(PropertyIdentifier::ObjectList, None)
        {
            return list
                .into_iter()
                .filter_map(|v| {
                    if let bacnet_types::property_value::PropertyValue::ObjectId(o) = v {
                        Some(ObjectListItem {
                            object_type: format!("{:?}", o.object_type),
                            instance: o.instance,
                        })
                    } else {
                        None
                    }
                })
                .collect();
        }
    }
    Vec::new()
}

fn parse_object_type(s: &str) -> Result<ObjectType, ApiError> {
    match s.to_lowercase().replace('-', "_").as_str() {
        "analog_input" | "analoginput" => Ok(ObjectType::AnalogInput),
        "analog_output" | "analogoutput" => Ok(ObjectType::AnalogOutput),
        "analog_value" | "analogvalue" => Ok(ObjectType::AnalogValue),
        "binary_input" | "binaryinput" => Ok(ObjectType::BinaryInput),
        "binary_output" | "binaryoutput" => Ok(ObjectType::BinaryOutput),
        "binary_value" | "binaryvalue" => Ok(ObjectType::BinaryValue),
        "device" => Ok(ObjectType::Device),
        "multi_state_input" | "multistateinput" => Ok(ObjectType::MultiStateInput),
        "multi_state_output" | "multistateoutput" => Ok(ObjectType::MultiStateOutput),
        "multi_state_value" | "multistatevalue" => Ok(ObjectType::MultiStateValue),
        "notification_class" | "notificationclass" => Ok(ObjectType::NotificationClass),
        "schedule" => Ok(ObjectType::Schedule),
        "trend_log" | "trendlog" => Ok(ObjectType::TrendLog),
        _ => Err(ApiError(
            StatusCode::BAD_REQUEST,
            format!("Unknown object type: {s}"),
        )),
    }
}

fn parse_property_id(s: &str) -> Result<bacnet_types::property_id::PropertyIdentifier, ApiError> {
    use bacnet_types::property_id::PropertyIdentifier;
    match s.to_lowercase().replace('-', "_").as_str() {
        "present_value" | "presentvalue" => Ok(PropertyIdentifier::PresentValue),
        "object_name" | "objectname" => Ok(PropertyIdentifier::ObjectName),
        "description" => Ok(PropertyIdentifier::Description),
        "out_of_service" | "outofservice" => Ok(PropertyIdentifier::OutOfService),
        "units" => Ok(PropertyIdentifier::Units),
        "status_flags" | "statusflags" => Ok(PropertyIdentifier::StatusFlags),
        _ => Err(ApiError(
            StatusCode::BAD_REQUEST,
            format!("Unknown property: {s}"),
        )),
    }
}

fn property_to_json(v: bacnet_types::property_value::PropertyValue) -> serde_json::Value {
    use bacnet_types::property_value::PropertyValue;
    match v {
        PropertyValue::Null => serde_json::Value::Null,
        PropertyValue::Boolean(b) => serde_json::json!(b),
        PropertyValue::Unsigned(n) => serde_json::json!(n),
        PropertyValue::Integer(n) => serde_json::json!(n),
        PropertyValue::Real(f) => serde_json::json!(f),
        PropertyValue::Double(f) => serde_json::json!(f),
        PropertyValue::CharacterString(s) => serde_json::json!(s),
        PropertyValue::Enumerated(n) => serde_json::json!(n),
        PropertyValue::ObjectId(o) => serde_json::json!({
            "type": format!("{:?}", o.object_type),
            "instance": o.instance,
        }),
        PropertyValue::Array(arr) => {
            serde_json::json!(arr.into_iter().map(property_to_json).collect::<Vec<_>>())
        }
        PropertyValue::List(lst) => {
            serde_json::json!(lst.into_iter().map(property_to_json).collect::<Vec<_>>())
        }
        _ => serde_json::json!("<binary>"),
    }
}

fn json_to_property_value(
    v: &serde_json::Value,
    _hint: bacnet_types::property_id::PropertyIdentifier,
) -> Result<bacnet_types::property_value::PropertyValue, String> {
    use bacnet_types::property_value::PropertyValue;
    match v {
        serde_json::Value::Null => Ok(PropertyValue::Null),
        serde_json::Value::Bool(b) => Ok(PropertyValue::Boolean(*b)),
        serde_json::Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                Ok(PropertyValue::Real(f as f32))
            } else {
                Err("Expected numeric value".into())
            }
        }
        serde_json::Value::String(s) => Ok(PropertyValue::CharacterString(s.clone())),
        _ => Err(format!("Unsupported JSON value type: {v:?}")),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt; // for `oneshot`

    fn make_app() -> (AppState, Router) {
        let store = Arc::new(ObjectStore::new());
        let state = AppState::new(store);
        let router = Router::new()
            .route("/api/v1/health", get(health))
            .route("/api/v1/metrics", get(metrics_handler))
            .route("/api/v1/devices", get(list_devices))
            .route("/api/v1/devices/{id}", get(get_device))
            .route("/api/v1/devices/{id}/objects", get(list_objects))
            .with_state(state.clone());
        (state, router)
    }

    #[tokio::test]
    async fn health_returns_ok() {
        let (_, app) = make_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn list_devices_empty() {
        let (_, app) = make_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/devices")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let items: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert!(items.is_empty());
    }

    #[tokio::test]
    async fn get_device_not_found() {
        let (_, app) = make_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/devices/9999")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn metrics_endpoint_returns_text() {
        let (_, app) = make_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
