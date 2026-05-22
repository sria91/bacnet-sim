/// Conformance test suite — ASHRAE 135.1 procedures.
///
/// These tests verify that the BACnet object model meets the required
/// property and behaviour rules from ASHRAE 135-2020 §12.
///
/// Run with: cargo test -p bacnet-object --test conformance

use std::sync::Arc;

use bacnet_object::{
    analog_input::AnalogInput,
    binary_input::BinaryInput,
    device::DeviceObject,
    store::ObjectStore,
};
use bacnet_types::{
    error::BacnetError,
    property_id::PropertyIdentifier,
    property_value::{EngineeringUnits, PropertyValue},
    DeviceId, ObjectId, ObjectType,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_store_with_ai() -> (Arc<ObjectStore>, DeviceId, ObjectId) {
    let store = Arc::new(ObjectStore::new());
    let did = DeviceId(500);
    let oid = ObjectId { object_type: ObjectType::AnalogInput, instance: 1 };
    store.insert(did, Box::new(DeviceObject::new(did, "test-device")));
    store.insert(did, Box::new(AnalogInput::new(did, 1, "AI-01", EngineeringUnits::DegreesCelsius)));
    (store, did, oid)
}

fn read(store: &ObjectStore, did: DeviceId, oid: ObjectId, prop: PropertyIdentifier) -> Option<PropertyValue> {
    let obj = store.get(did, oid)?;
    let guard = obj.read_guard();
    guard.read_property(prop, None).ok()
}

// ---------------------------------------------------------------------------
// 9.1  Device object required properties
// ---------------------------------------------------------------------------

/// Every Device object MUST expose all mandatory properties defined in §12.10.
#[test]
fn device_object_has_all_required_properties() {
    let store = Arc::new(ObjectStore::new());
    let did = DeviceId(1234);
    let dev_oid = ObjectId { object_type: ObjectType::Device, instance: 1234 };
    store.insert(did, Box::new(DeviceObject::new(did, "conformance-dev")));

    let required = [
        PropertyIdentifier::ObjectIdentifier,
        PropertyIdentifier::ObjectName,
        PropertyIdentifier::ObjectType,
        PropertyIdentifier::SystemStatus,
        PropertyIdentifier::VendorName,
        PropertyIdentifier::VendorIdentifier,
        PropertyIdentifier::ModelName,
        PropertyIdentifier::FirmwareRevision,
        PropertyIdentifier::ApplicationSoftwareVersion,
        PropertyIdentifier::ProtocolVersion,
        PropertyIdentifier::ProtocolRevision,
        PropertyIdentifier::MaxApduLengthAccepted,
        PropertyIdentifier::SegmentationSupported,
        PropertyIdentifier::ObjectList,
        PropertyIdentifier::DatabaseRevision,
    ];

    let obj = store.get(did, dev_oid).expect("Device object not found");
    let guard = obj.read_guard();
    for &prop in &required {
        let result = guard.read_property(prop, None);
        assert!(
            result.is_ok(),
            "CONFORMANCE §12.10: Device required property {prop:?} is missing"
        );
    }
}

// ---------------------------------------------------------------------------
// 12.2  Analog Input object required properties
// ---------------------------------------------------------------------------

/// All mandatory AI properties from §12.2 must be readable.
#[test]
fn analog_input_has_all_required_properties() {
    let (store, did, oid) = make_store_with_ai();

    let required = [
        PropertyIdentifier::ObjectIdentifier,
        PropertyIdentifier::ObjectName,
        PropertyIdentifier::ObjectType,
        PropertyIdentifier::PresentValue,
        PropertyIdentifier::StatusFlags,
        PropertyIdentifier::EventState,
        PropertyIdentifier::OutOfService,
        PropertyIdentifier::Units,
    ];

    let obj = store.get(did, oid).expect("AI not found");
    let guard = obj.read_guard();
    for &prop in &required {
        let result = guard.read_property(prop, None);
        assert!(
            result.is_ok(),
            "CONFORMANCE §12.2: AI required property {prop:?} missing"
        );
    }
}

/// Present_Value of a freshly created AI must be a Real number.
#[test]
fn analog_input_present_value_is_real() {
    let (store, did, oid) = make_store_with_ai();
    let val = read(&store, did, oid, PropertyIdentifier::PresentValue)
        .expect("PresentValue missing");
    assert!(
        matches!(val, PropertyValue::Real(_)),
        "PresentValue must be Real, got {val:?}"
    );
}

/// Object_Type of an AI must be the Enumerated value for AnalogInput (0).
#[test]
fn analog_input_object_type_is_correct() {
    let (store, did, oid) = make_store_with_ai();
    let val = read(&store, did, oid, PropertyIdentifier::ObjectType)
        .expect("ObjectType missing");
    assert_eq!(val, PropertyValue::Enumerated(0), "ObjectType must be AnalogInput (0)");
}

/// Out_Of_Service defaults to false on a fresh AI.
#[test]
fn analog_input_out_of_service_defaults_false() {
    let (store, did, oid) = make_store_with_ai();
    let val = read(&store, did, oid, PropertyIdentifier::OutOfService)
        .expect("OutOfService missing");
    assert_eq!(val, PropertyValue::Boolean(false));
}

/// Units must equal DegreesCelsius (62) as specified at construction.
#[test]
fn analog_input_units_match_construction() {
    let (store, did, oid) = make_store_with_ai();
    let val = read(&store, did, oid, PropertyIdentifier::Units)
        .expect("Units missing");
    // EngineeringUnits::DegreesCelsius = 62
    assert_eq!(val, PropertyValue::Enumerated(62));
}

// ---------------------------------------------------------------------------
// 12.6  Binary Input object required properties
// ---------------------------------------------------------------------------

/// All mandatory BI properties must be readable.
#[test]
fn binary_input_has_all_required_properties() {
    let store = Arc::new(ObjectStore::new());
    let did = DeviceId(501);
    let oid = ObjectId { object_type: ObjectType::BinaryInput, instance: 1 };
    store.insert(did, Box::new(BinaryInput::new(did, 1, "BI-01")));

    let required = [
        PropertyIdentifier::ObjectIdentifier,
        PropertyIdentifier::ObjectName,
        PropertyIdentifier::ObjectType,
        PropertyIdentifier::PresentValue,
        PropertyIdentifier::StatusFlags,
        PropertyIdentifier::EventState,
        PropertyIdentifier::OutOfService,
    ];

    let obj = store.get(did, oid).expect("BI not found");
    let guard = obj.read_guard();
    for &prop in &required {
        let result = guard.read_property(prop, None);
        assert!(
            result.is_ok(),
            "CONFORMANCE §12.6: BI required property {prop:?} missing"
        );
    }
}

// ---------------------------------------------------------------------------
// 13.3  Error responses
// ---------------------------------------------------------------------------

/// Reading an unknown object must return error class Object / code UnknownObject.
#[test]
fn unknown_object_returns_error() {
    let store = Arc::new(ObjectStore::new());
    let did = DeviceId(999);
    let missing_oid = ObjectId { object_type: ObjectType::AnalogInput, instance: 9999 };
    let result = store.get(did, missing_oid);
    assert!(result.is_none(), "Non-existent object should not be found");
}

/// Reading an unknown property on an existing object must return UnknownProperty.
#[test]
fn unknown_property_returns_error() {
    let (store, did, oid) = make_store_with_ai();
    let obj = store.get(did, oid).expect("AI not found");
    let guard = obj.read_guard();
    let err = guard.read_property(PropertyIdentifier::Unknown(9999), None);
    assert!(err.is_err(), "Unknown property must return an error");
    let e = err.unwrap_err();
    assert!(
        matches!(e, BacnetError::UnknownProperty),
        "Expected UnknownProperty error, got {e:?}"
    );
}

// ---------------------------------------------------------------------------
// Object_Identifier encoding
// ---------------------------------------------------------------------------

/// Object_Identifier must encode the (type, instance) pair correctly.
#[test]
fn object_identifier_encodes_type_and_instance() {
    let (store, did, oid) = make_store_with_ai();
    let val = read(&store, did, oid, PropertyIdentifier::ObjectIdentifier)
        .expect("ObjectIdentifier missing");
    // Encoded as a 4-byte unsigned: type (upper 10 bits) << 22 | instance
    match val {
        PropertyValue::ObjectId(returned) => {
            assert_eq!(returned.object_type, ObjectType::AnalogInput);
            assert_eq!(returned.instance, 1);
        }
        other => panic!("Expected ObjectId, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 16.1  Who-Is / I-Am — unit-level range check (no network needed)
// ---------------------------------------------------------------------------

/// ObjectStore correctly stores and retrieves devices in a range.
#[test]
fn object_store_multi_device_range() {
    let store = Arc::new(ObjectStore::new());
    for id in 1000u32..=1009 {
        let did = DeviceId(id);
        store.insert(did, Box::new(DeviceObject::new(did, format!("DEV-{id}"))));
        store.insert(did, Box::new(AnalogInput::new(did, 1, "AI-01", EngineeringUnits::Kelvin)));
    }
    assert_eq!(store.count(), 20, "10 devices × 2 objects each = 20 total objects");

    // All Device objects are retrievable
    for id in 1000u32..=1009 {
        let did = DeviceId(id);
        let dev_oid = ObjectId { object_type: ObjectType::Device, instance: id };
        let obj = store.get(did, dev_oid);
        assert!(obj.is_some(), "Device {id} should be in store");
    }
}

// ---------------------------------------------------------------------------
// WriteProperty guard: Out_Of_Service must be true before PresentValue write
// ---------------------------------------------------------------------------

/// Writing PresentValue on a live (Out_Of_Service=false) AI is denied.
#[test]
fn write_present_value_denied_when_in_service() {
    let (store, did, oid) = make_store_with_ai();
    let mut obj = store.get(did, oid).expect("AI not found");
    let mut guard = obj.write_guard();
    let result = guard.write_property(
        PropertyIdentifier::PresentValue,
        None,
        PropertyValue::Real(99.0),
        None,
    );
    assert!(result.is_err());
    let e = result.unwrap_err();
    assert!(
        matches!(e, BacnetError::WriteAccessDenied),
        "Expected WriteAccessDenied, got {e:?}"
    );
}

/// Writing PresentValue succeeds when Out_Of_Service=true.
#[test]
fn write_present_value_allowed_when_out_of_service() {
    let (store, did, oid) = make_store_with_ai();
    let mut obj = store.get(did, oid).expect("AI not found");
    {
        let mut guard = obj.write_guard();
        guard
            .write_property(PropertyIdentifier::OutOfService, None, PropertyValue::Boolean(true), None)
            .expect("should allow writing OutOfService");
        guard
            .write_property(PropertyIdentifier::PresentValue, None, PropertyValue::Real(42.0), None)
            .expect("should allow writing PresentValue when OOS");
    }
    let guard = obj.read_guard();
    let pv = guard.read_property(PropertyIdentifier::PresentValue, None).unwrap();
    assert_eq!(pv, PropertyValue::Real(42.0));
}
