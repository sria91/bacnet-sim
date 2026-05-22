use bacnet_codec::apdu::confirmed::WritePropertyRequest;
use bacnet_object::store::ObjectStore;
use bacnet_types::{DeviceId, error::BacnetError};

pub async fn handle_write_property(
    req: WritePropertyRequest,
    store: &ObjectStore,
    device_id: DeviceId,
) -> Result<(), BacnetError> {
    let mut obj = store
        .get(device_id, req.object_id)
        .ok_or(BacnetError::UnknownObject)?;

    let mut guard = obj.write_guard();
    guard.write_property(req.property_id, req.array_index, req.value, req.priority)
}
