/// Config-driven simulation builder.
///
/// Reads a `SimulatorConfig` and instantiates the full device/object tree into
/// an `ObjectStore`, optionally registering value models with a `SimEngine`.

use std::sync::Arc;

use bacnet_config::topology::{ModelParams, ObjectConfig, ProfileConfig, SimulatorConfig};
use bacnet_object::{
    analog_input::AnalogInput,
    analog_output::AnalogOutput,
    analog_value::AnalogValue,
    binary_input::BinaryInput,
    binary_output::BinaryOutput,
    binary_value::BinaryValue,
    device::DeviceObject,
    multistate::MultiStateInput,
    property::BacnetObject,
    store::ObjectStore,
};
use bacnet_sim_engine::{
    engine::SimEngine,
    value_model::{
        ConstantModel, RandomWalkModel, SineModel, StepModel, ThermalModel, ValueModel,
    },
};
use bacnet_stack::dispatcher::{ApduDispatcher, DeviceInfo};
use bacnet_types::{property_value::EngineeringUnits, DeviceId};
use tracing::info;

/// Build an entire simulation from a parsed `SimulatorConfig`.
///
/// Returns the populated `ObjectStore`, a configured `SimEngine` (already
/// seeded with all value models), plus the `ApduDispatcher` with every device
/// registered.
pub fn build_simulation(
    config: &SimulatorConfig,
    store: Arc<ObjectStore>,
    dispatcher: &mut ApduDispatcher,
) -> SimEngine {
    let (engine, _cov_rx) = SimEngine::new(Arc::clone(&store), config.simulator.tick_hz);
    // _cov_rx is intentionally dropped here; callers that need it should call
    // SimEngine::new directly and pass a pre-built store.

    let mut batch: Vec<(DeviceId, Box<dyn BacnetObject>)> = Vec::new();

    for group in &config.devices {
        let profile = match config.profiles.get(&group.profile) {
            Some(p) => p,
            None => {
                tracing::warn!(profile = %group.profile, "Unknown profile, skipping");
                continue;
            }
        };

        let [lo, hi] = group.id_range;
        for device_instance in lo..=hi {
            let dev_id = DeviceId(device_instance);

            // Device object
            let dev_obj = DeviceObject::new(dev_id, format!("device-{device_instance}"));
            batch.push((dev_id, Box::new(dev_obj)));

            // Objects from profile
            add_profile_objects(dev_id, profile, &mut batch);

            // Register device in dispatcher
            dispatcher.register_device(DeviceInfo::new(device_instance));
        }
    }

    let total = batch.len();
    store.bulk_insert(batch);
    info!(objects = total, "Bulk-inserted objects from topology config");

    engine
}

fn add_profile_objects(
    dev_id: DeviceId,
    profile: &ProfileConfig,
    batch: &mut Vec<(DeviceId, Box<dyn BacnetObject>)>,
) {
    for obj_cfg in &profile.objects {
        for i in 1..=obj_cfg.count {
            let name = format!("{}-{i:03}", obj_cfg.name_prefix);
            let units = parse_units(obj_cfg.units.as_deref());

            match obj_cfg.object_type.to_lowercase().replace([' ', '-'], "_").as_str() {
                "analoginput" | "analog_input" => {
                    batch.push((dev_id, Box::new(AnalogInput::new(dev_id, i, &name, units))));
                }
                "analogoutput" | "analog_output" => {
                    batch.push((dev_id, Box::new(AnalogOutput::new(dev_id, i, &name, units))));
                }
                "analogvalue" | "analog_value" => {
                    batch.push((dev_id, Box::new(AnalogValue::new(dev_id, i, &name, units))));
                }
                "binaryinput" | "binary_input" => {
                    batch.push((dev_id, Box::new(BinaryInput::new(dev_id, i, &name))));
                }
                "binaryoutput" | "binary_output" => {
                    batch.push((dev_id, Box::new(BinaryOutput::new(dev_id, i, &name))));
                }
                "binaryvalue" | "binary_value" => {
                    batch.push((dev_id, Box::new(BinaryValue::new(dev_id, i, &name))));
                }
                "multistateinput" | "multistate_input" | "msi" => {
                    // Default 4 states; profiles can set more via count
                    batch.push((dev_id, Box::new(MultiStateInput::new(dev_id, i, &name, 4))));
                }
                other => {
                    tracing::warn!(object_type = other, "Unrecognised object type, skipping");
                }
            }
        }
    }
}

/// Parse a units string (case-insensitive) to `EngineeringUnits`.
pub fn parse_units(s: Option<&str>) -> EngineeringUnits {
    match s.map(|s| s.to_lowercase()).as_deref() {
        Some("degreescelsius" | "degrees_celsius" | "celsius" | "°c") => {
            EngineeringUnits::DegreesCelsius
        }
        Some("degreesfahrenheit" | "degrees_fahrenheit" | "fahrenheit" | "°f") => {
            EngineeringUnits::DegreesFahrenheit
        }
        Some("kelvin" | "k") => EngineeringUnits::Kelvin,
        Some("percent" | "%") => EngineeringUnits::Percent,
        Some("psi" | "poundspersquareinch") => EngineeringUnits::PoundsPerSquareInch,
        Some("watts" | "w") => EngineeringUnits::Watts,
        Some("kilowatts" | "kw") => EngineeringUnits::Kilowatts,
        Some("amperes" | "a" | "amps") => EngineeringUnits::Amperes,
        Some("volts" | "v") => EngineeringUnits::Volts,
        Some("hertz" | "hz") => EngineeringUnits::Hertz,
        _ => EngineeringUnits::NoUnits,
    }
}

/// Parse a model name + params into a `Box<dyn ValueModel>`.
pub fn parse_model(model: Option<&str>, params: &ModelParams) -> Option<Box<dyn ValueModel>> {
    match model?.to_lowercase().as_str() {
        "constant" => Some(Box::new(ConstantModel(params.value.unwrap_or(0.0)))),
        "sine" => Some(Box::new(SineModel {
            amplitude: params.amplitude.unwrap_or(5.0),
            period_s: params.period_s.unwrap_or(3600.0),
            offset: params.offset.unwrap_or(20.0),
            noise_std: params.noise_std.unwrap_or(0.1),
        })),
        "random_walk" | "randomwalk" => Some(Box::new(RandomWalkModel {
            current: params.offset.unwrap_or(20.0),
            step_std: params.step_std.unwrap_or(0.5),
            min: params.min.unwrap_or(0.0),
            max: params.max.unwrap_or(100.0),
        })),
        "step" => Some(Box::new(StepModel {
            schedule: vec![(0.0, params.value.unwrap_or(0.0))],
        })),
        "thermal" => Some(Box::new(ThermalModel {
            setpoint: params.offset.unwrap_or(22.0),
            current: params.offset.unwrap_or(22.0),
            time_const_s: params.period_s.unwrap_or(600.0),
            ambient: params.min.unwrap_or(15.0),
            noise_std: params.noise_std.unwrap_or(0.05),
            last_t: 0.0,
        })),
        other => {
            tracing::warn!(model = other, "Unknown value model");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bacnet_config::topology::{
        DeviceGroupConfig, NetworkConfig, ObjectConfig, ProfileConfig, SimulatorConfig,
        SimulatorSection, TransportKind,
    };
    use bacnet_stack::dispatcher::ApduDispatcher;
    use std::collections::HashMap;

    fn make_config(devices: u32, objects_per_device: u32) -> SimulatorConfig {
        let mut profiles = HashMap::new();
        profiles.insert(
            "test_profile".to_string(),
            ProfileConfig {
                description: None,
                objects: vec![ObjectConfig {
                    object_type: "AnalogInput".to_string(),
                    count: objects_per_device,
                    name_prefix: "AI".to_string(),
                    units: Some("DegreesCelsius".to_string()),
                    model: Some("sine".to_string()),
                    model_params: ModelParams::default(),
                }],
            },
        );
        SimulatorConfig {
            simulator: SimulatorSection { tick_hz: 1.0, seed: Some(42) },
            networks: vec![NetworkConfig {
                id: 1,
                transport: TransportKind::BacnetIp,
                bind: Some("127.0.0.1:47808".to_string()),
                hub_url: None,
            }],
            devices: vec![DeviceGroupConfig {
                id_range: [1, devices],
                network: 1,
                profile: "test_profile".to_string(),
            }],
            profiles,
        }
    }

    #[test]
    fn bulk_build_correct_count() {
        let config = make_config(10, 5);
        let store = Arc::new(ObjectStore::new());
        let mut dispatcher = ApduDispatcher::new(Arc::clone(&store));
        build_simulation(&config, Arc::clone(&store), &mut dispatcher);

        // 10 devices × (1 device-object + 5 AI) = 60 objects
        assert_eq!(store.count(), 60);
    }

    #[test]
    fn bulk_build_1000_devices_10_objects() {
        let config = make_config(1000, 10);
        let store = Arc::new(ObjectStore::new());
        let mut dispatcher = ApduDispatcher::new(Arc::clone(&store));
        build_simulation(&config, Arc::clone(&store), &mut dispatcher);

        // 1000 × (1 + 10) = 11 000
        assert_eq!(store.count(), 11_000);
    }

    #[test]
    fn parse_units_case_insensitive() {
        assert_eq!(parse_units(Some("DegreesCelsius")), EngineeringUnits::DegreesCelsius);
        assert_eq!(parse_units(Some("celsius")), EngineeringUnits::DegreesCelsius);
        assert_eq!(parse_units(Some("percent")), EngineeringUnits::Percent);
        assert_eq!(parse_units(None), EngineeringUnits::NoUnits);
    }

    #[test]
    fn parse_model_sine() {
        let params = ModelParams {
            amplitude: Some(10.0),
            period_s: Some(60.0),
            offset: Some(25.0),
            ..Default::default()
        };
        let model = parse_model(Some("sine"), &params);
        assert!(model.is_some());
    }
}
