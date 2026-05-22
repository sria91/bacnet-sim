/// Simulator topology definition — parsed from TOML / YAML / JSON.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SimulatorConfig {
    pub simulator: SimulatorSection,
    #[serde(default)]
    pub networks: Vec<NetworkConfig>,
    #[serde(default)]
    pub devices: Vec<DeviceGroupConfig>,
    #[serde(default)]
    pub profiles: std::collections::HashMap<String, ProfileConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SimulatorSection {
    pub tick_hz: f64,
    pub seed: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NetworkConfig {
    pub id: u16,
    pub transport: TransportKind,
    pub bind: Option<String>,
    pub hub_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportKind {
    BacnetIp,
    Mstp,
    BacnetSc,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DeviceGroupConfig {
    pub id_range: [u32; 2],
    pub network: u16,
    pub profile: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProfileConfig {
    pub description: Option<String>,
    pub objects: Vec<ObjectConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ObjectConfig {
    #[serde(rename = "type")]
    pub object_type: String,
    pub count: u32,
    pub name_prefix: String,
    pub units: Option<String>,
    pub model: Option<String>,
    /// Optional model parameters.
    #[serde(default)]
    pub model_params: ModelParams,
}

/// Simple flat model parameters (all optional with sensible defaults).
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ModelParams {
    pub amplitude: Option<f32>,
    pub period_s: Option<f64>,
    pub offset: Option<f32>,
    pub noise_std: Option<f32>,
    pub step_std: Option<f32>,
    pub min: Option<f32>,
    pub max: Option<f32>,
    pub value: Option<f32>,
}

impl SimulatorConfig {
    pub fn from_toml(s: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(s)
    }

    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }
}
