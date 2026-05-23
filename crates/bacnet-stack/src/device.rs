/// Virtual device handler — processes APDU service requests for one simulated device.

use bacnet_types::DeviceId;

/// Placeholder trait for a device's APDU service handler.
pub trait DeviceHandler: Send + Sync {
    fn device_id(&self) -> DeviceId;
    fn handle_who_is(&self, low: Option<u32>, high: Option<u32>) -> bool;
}
