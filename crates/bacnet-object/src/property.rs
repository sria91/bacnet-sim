use bacnet_types::{
    DeviceId, ObjectId, PropertyIdentifier, PropertyValue,
    error::BacnetError,
};
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
}
