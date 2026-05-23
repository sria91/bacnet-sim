/// Schedule engine: evaluates BACnet Schedule objects each tick and writes
/// their present_value to linked object-property references.
use bacnet_object::store::ObjectStore;
use bacnet_types::{DeviceId, ObjectId, PropertyIdentifier, PropertyValue};
use std::sync::Arc;
use std::time::SystemTime;

/// A registered schedule output binding.
pub struct ScheduleOutput {
    pub schedule_device: DeviceId,
    pub schedule_object: ObjectId,
    pub target_device: DeviceId,
    pub target_object: ObjectId,
    pub target_property: PropertyIdentifier,
}

pub struct ScheduleEngine {
    store: Arc<ObjectStore>,
    outputs: Vec<ScheduleOutput>,
}

impl ScheduleEngine {
    pub fn new(store: Arc<ObjectStore>) -> Self {
        Self {
            store,
            outputs: Vec::new(),
        }
    }

    pub fn add_output(&mut self, output: ScheduleOutput) {
        self.outputs.push(output);
    }

    /// Drive all schedules: read current present_value and propagate to targets.
    pub fn tick(&self, _now: SystemTime) {
        for out in &self.outputs {
            let pv = match self.store.get(out.schedule_device, out.schedule_object) {
                Some(obj_ref) => obj_ref
                    .read_guard()
                    .read_property(PropertyIdentifier::PresentValue, None)
                    .ok(),
                None => continue,
            };
            let pv = match pv {
                Some(v) if v != PropertyValue::Null => v,
                _ => continue,
            };
            if let Some(target_ref) = self.store.get(out.target_device, out.target_object) {
                let _ = target_ref.write_property_once(out.target_property, None, pv, None);
            }
        }
    }
}
