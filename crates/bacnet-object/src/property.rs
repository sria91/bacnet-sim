use bacnet_types::{error::BacnetError, DeviceId, ObjectId, PropertyIdentifier, PropertyValue};
use std::time::{Duration, Instant, SystemTime};

/// Core trait every BACnet object must implement.
pub trait BacnetObject: Send + Sync {
    fn object_id(&self) -> ObjectId;
    fn device_id(&self) -> DeviceId;

    fn read_property(
        &self,
        property_id: PropertyIdentifier,
        array_index: Option<u32>,
    ) -> Result<PropertyValue, BacnetError>;

    fn write_property(
        &mut self,
        property_id: PropertyIdentifier,
        array_index: Option<u32>,
        value: PropertyValue,
        priority: Option<u8>,
    ) -> Result<(), BacnetError>;

    /// Called every simulation tick.
    fn tick(&mut self, now: SystemTime, delta: Duration);

    /// Returns properties that have changed since `since` (for COV).
    fn changed_since(&self, since: Instant) -> Vec<PropertyIdentifier>;

    /// Returns all readable properties with their current values (for RPM `ALL`).
    fn all_properties(&self) -> Vec<(PropertyIdentifier, PropertyValue)>;

    /// Simulation-internal write: bypasses out-of-service guards.
    /// Used by the tick loop and scenario runner to force property values.
    fn force_write_property(
        &mut self,
        property_id: PropertyIdentifier,
        value: PropertyValue,
    ) -> Result<(), BacnetError> {
        // Try normal write first (works for properties without OOS guard).
        if self
            .write_property(property_id, None, value.clone(), None)
            .is_ok()
        {
            return Ok(());
        }
        // For PresentValue, temporarily enable out-of-service to bypass the guard.
        if property_id == PropertyIdentifier::PresentValue {
            let was_oos = matches!(
                self.read_property(PropertyIdentifier::OutOfService, None),
                Ok(PropertyValue::Boolean(true))
            );
            if !was_oos {
                let _ = self.write_property(
                    PropertyIdentifier::OutOfService,
                    None,
                    PropertyValue::Boolean(true),
                    None,
                );
                let result = self.write_property(property_id, None, value, None);
                let _ = self.write_property(
                    PropertyIdentifier::OutOfService,
                    None,
                    PropertyValue::Boolean(false),
                    None,
                );
                return result;
            }
        }
        Err(BacnetError::WriteAccessDenied)
    }
}
