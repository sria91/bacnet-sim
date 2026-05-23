use bacnet_codec::apdu::ack::{
    ComplexAck, ComplexAckService, ObjectPropertyResult, PropertyResult,
};
use bacnet_object::store::ObjectStore;
use bacnet_types::{error::BacnetError, DeviceId, ObjectId, PropertyIdentifier};

pub async fn handle_read_property_multiple(
    specs: Vec<(ObjectId, Vec<(PropertyIdentifier, Option<u32>)>)>,
    store: &ObjectStore,
    device_id: DeviceId,
    invoke_id: u8,
) -> ComplexAck {
    let mut results = Vec::with_capacity(specs.len());
    for (object_id, props) in specs {
        let mut property_results = Vec::with_capacity(props.len());
        match store.get(device_id, object_id) {
            None => {
                for (prop_id, array_index) in props {
                    property_results.push(PropertyResult {
                        property_id: prop_id,
                        array_index,
                        value: Err(BacnetError::UnknownObject),
                    });
                }
            }
            Some(obj) => {
                let guard = obj.read_guard();
                let has_all = props.iter().any(|(p, _)| *p == PropertyIdentifier::All);
                if has_all {
                    property_results.extend(guard.all_properties().into_iter().map(|(pid, v)| {
                        PropertyResult {
                            property_id: pid,
                            array_index: None,
                            value: Ok(v),
                        }
                    }));
                } else {
                    for (prop_id, array_index) in props {
                        let value = guard.read_property(prop_id, array_index);
                        property_results.push(PropertyResult {
                            property_id: prop_id,
                            array_index,
                            value,
                        });
                    }
                }
            }
        }
        results.push(ObjectPropertyResult {
            object_id,
            property_results,
        });
    }
    ComplexAck {
        invoke_id,
        service: ComplexAckService::ReadPropertyMultiple(results),
    }
}
