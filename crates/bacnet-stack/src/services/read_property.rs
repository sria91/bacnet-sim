use bacnet_codec::apdu::{
    ack::{ReadPropertyAck, ComplexAck, ComplexAckService},
    confirmed::ReadPropertyRequest,
};
use bacnet_object::store::ObjectStore;
use bacnet_types::{DeviceId, error::BacnetError};

pub async fn handle_read_property(
    req: ReadPropertyRequest,
    store: &ObjectStore,
    device_id: DeviceId,
) -> Result<ComplexAck, BacnetError> {
    let invoke_id = 0; // caller fills this in
    let obj = store
        .get(device_id, req.object_id)
        .ok_or(BacnetError::UnknownObject)?;

    let value = {
        let guard = obj.read_guard();
        guard.read_property(req.property_id, req.array_index)?
    };

    Ok(ComplexAck {
        invoke_id,
        service: ComplexAckService::ReadProperty(ReadPropertyAck {
            object_id: req.object_id,
            property_id: req.property_id,
            array_index: req.array_index,
            value,
        }),
    })
}
