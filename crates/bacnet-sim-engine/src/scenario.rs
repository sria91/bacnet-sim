/// Scenario scripting: loads and runs Rhai scripts that manipulate the simulation.

use bacnet_object::store::ObjectStore;
use bacnet_types::{DeviceId, ObjectId, PropertyIdentifier, PropertyValue};
use rhai::{Engine, Scope, AST};
use std::path::Path;
use std::sync::Arc;

/// A loaded scenario script with its compiled AST.
pub struct Scenario {
    pub name: String,
    ast: AST,
}

pub struct ScenarioRunner {
    engine: Engine,
    #[allow(dead_code)]
    store: Arc<ObjectStore>,
}

impl ScenarioRunner {
    pub fn new(store: Arc<ObjectStore>) -> Self {
        let mut engine = Engine::new();

        // set_analog(device_id, object_type, instance, value)
        let store_a = store.clone();
        engine.register_fn(
            "set_analog",
            move |device_id: i64, object_type: i64, instance: i64, value: f64| {
                let dev = DeviceId(device_id as u32);
                let oid = ObjectId {
                    object_type: bacnet_types::ObjectType::from_u16(object_type as u16)
                        .unwrap_or(bacnet_types::ObjectType::AnalogInput),
                    instance: instance as u32,
                };
                if let Some(obj_ref) = store_a.get(dev, oid) {
                    let _ = obj_ref.force_write_property_once(
                        PropertyIdentifier::PresentValue,
                        PropertyValue::Real(value as f32),
                    );
                }
            },
        );

        // set_binary(device_id, object_type, instance, value)
        let store_b = store.clone();
        engine.register_fn(
            "set_binary",
            move |device_id: i64, object_type: i64, instance: i64, value: bool| {
                let dev = DeviceId(device_id as u32);
                let oid = ObjectId {
                    object_type: bacnet_types::ObjectType::from_u16(object_type as u16)
                        .unwrap_or(bacnet_types::ObjectType::BinaryInput),
                    instance: instance as u32,
                };
                if let Some(obj_ref) = store_b.get(dev, oid) {
                    let _ = obj_ref.force_write_property_once(
                        PropertyIdentifier::PresentValue,
                        PropertyValue::Boolean(value),
                    );
                }
            },
        );

        Self { engine, store }
    }

    /// Load a scenario from a Rhai source file.
    pub fn load_file(&self, path: &Path) -> Result<Scenario, Box<dyn std::error::Error>> {
        let source = std::fs::read_to_string(path)?;
        self.load_str(
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unnamed"),
            &source,
        )
    }

    /// Load a scenario from a Rhai source string.
    pub fn load_str(&self, name: &str, source: &str) -> Result<Scenario, Box<dyn std::error::Error>> {
        let ast = self.engine.compile(source)?;
        Ok(Scenario { name: name.to_string(), ast })
    }

    /// Execute a scenario at simulation time `t` (seconds since start).
    pub fn run(&self, scenario: &Scenario, t: f64) -> Result<(), Box<dyn std::error::Error>> {
        let mut scope = Scope::new();
        scope.push("t", t);
        self.engine.run_ast_with_scope(&mut scope, &scenario.ast)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bacnet_object::analog_input::AnalogInput;
    use bacnet_types::{property_value::EngineeringUnits, ObjectType};

    #[test]
    fn scenario_set_analog_value() {
        let store = Arc::new(ObjectStore::new());
        let dev = DeviceId(1);
        let ai = AnalogInput::new(dev, 1, "AI-1", EngineeringUnits::DegreesCelsius);
        store.insert(dev, Box::new(ai));

        let runner = ScenarioRunner::new(store.clone());
        let scenario = runner
            .load_str("test", "set_analog(1, 0, 1, 42.5)")
            .expect("compile failed");
        runner.run(&scenario, 0.0).expect("run failed");

        let obj_ref = store
            .get(dev, ObjectId { object_type: ObjectType::AnalogInput, instance: 1 })
            .unwrap();
        let val = obj_ref.read_guard()
            .read_property(PropertyIdentifier::PresentValue, None)
            .unwrap();
        assert_eq!(val, PropertyValue::Real(42.5));
    }
}
