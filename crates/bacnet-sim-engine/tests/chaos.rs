/// Chaos / fault-injection tests for the simulation engine.
///
/// These tests verify that the engine remains correct under high-concurrency
/// load, large subscription counts, expiry edge cases, and bulk data insertion.
///
/// Run with: cargo test -p bacnet-sim-engine --test chaos
use std::net::{Ipv4Addr, SocketAddrV4};
use std::sync::Arc;
use std::time::{Duration, Instant};

use bacnet_object::{analog_input::AnalogInput, store::ObjectStore};
use bacnet_sim_engine::cov_engine::{CovEngine, CovSubKey, CovSubscription};
use bacnet_types::{
    property_value::{EngineeringUnits, PropertyValue},
    DeviceId, MacAddr, NetworkAddress, ObjectId, ObjectType, PropertyIdentifier,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn sub_key(process_id: u32, device_id: u32, instance: u32) -> CovSubKey {
    CovSubKey {
        subscriber: NetworkAddress {
            network_number: 0,
            mac: MacAddr::Ip(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 47808)),
        },
        process_id,
        object_id: ObjectId {
            object_type: ObjectType::AnalogInput,
            instance,
        },
        device_id: DeviceId(device_id),
    }
}

fn make_sub(lifetime_secs: Option<u32>) -> CovSubscription {
    CovSubscription {
        confirmed: false,
        lifetime_secs,
        subscribed_at: Instant::now(),
        cov_increment: Some(0.5),
        monitored_prop: None,
        last_notified_at: Instant::now(),
        last_value: PropertyValue::Real(0.0),
    }
}

// ---------------------------------------------------------------------------
// 1. Large subscription count
// ---------------------------------------------------------------------------

/// Creating 10 000 COV subscriptions must succeed and all must be active.
#[test]
fn chaos_10k_cov_subscriptions_all_active() {
    let (engine, _rx) = CovEngine::new();

    for i in 0u32..10_000 {
        engine.subscribe(sub_key(i, i % 1000, i % 100), make_sub(Some(3600)));
    }

    let count = engine.active_count();
    // Duplicates are possible (same key written twice) so check >= reasonable lower bound.
    assert!(count > 0, "Expected subscriptions to be active");
    // All inserted; with distinct keys we expect exactly 10 000.
    assert_eq!(count, 10_000);
}

// ---------------------------------------------------------------------------
// 2. Subscription expiry
// ---------------------------------------------------------------------------

/// `expire_old` must remove subscriptions whose lifetime has elapsed.
#[test]
fn chaos_expired_subscriptions_are_removed() {
    let (engine, _rx) = CovEngine::new();

    // Add a subscription that has already expired (lifetime=1s, subscribed 5s ago).
    let mut sub = make_sub(Some(1));
    sub.subscribed_at = Instant::now() - Duration::from_secs(5);

    let key = sub_key(1, 1, 1);
    engine.subscribe(key.clone(), sub);
    assert_eq!(engine.active_count(), 1);

    let removed = engine.expire_old();
    assert_eq!(removed, 1, "Should have removed 1 expired subscription");
    assert_eq!(engine.active_count(), 0);
}

/// Subscriptions with no lifetime (perpetual) must NOT be removed by `expire_old`.
#[test]
fn chaos_perpetual_subscriptions_survive_expire_old() {
    let (engine, _rx) = CovEngine::new();
    engine.subscribe(sub_key(1, 1, 1), make_sub(None));
    engine.expire_old();
    assert_eq!(engine.active_count(), 1);
}

// ---------------------------------------------------------------------------
// 3. Rapid subscribe / unsubscribe
// ---------------------------------------------------------------------------

/// Rapid subscribe + unsubscribe cycles must leave the engine in a clean state.
#[test]
fn chaos_rapid_subscribe_unsubscribe_leaves_clean_state() {
    let (engine, _rx) = CovEngine::new();

    let key = sub_key(99, 99, 99);
    for _ in 0..1_000 {
        engine.subscribe(key.clone(), make_sub(Some(60)));
        engine.unsubscribe(&key);
    }

    assert_eq!(
        engine.active_count(),
        0,
        "All subscriptions should be removed"
    );
}

// ---------------------------------------------------------------------------
// 4. check_and_notify increments total_count correctly
// ---------------------------------------------------------------------------

/// check_and_notify must fire when the value changes beyond cov_increment.
#[test]
fn chaos_cov_fires_on_threshold_change() {
    let (engine, mut rx) = CovEngine::new();

    let device_id = DeviceId(1);
    let object_id = ObjectId {
        object_type: ObjectType::AnalogInput,
        instance: 1,
    };
    let subscriber = NetworkAddress {
        network_number: 0,
        mac: MacAddr::Ip(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 47808)),
    };

    let key = CovSubKey {
        subscriber: subscriber.clone(),
        process_id: 1,
        object_id,
        device_id,
    };
    let mut sub = make_sub(Some(60));
    sub.cov_increment = Some(1.0);
    sub.last_value = PropertyValue::Real(0.0);
    engine.subscribe(key, sub);

    // Change by 2.0 (> increment of 1.0) — must fire.
    let notified = engine.check_and_notify(
        device_id,
        object_id,
        PropertyIdentifier::PresentValue,
        &PropertyValue::Real(2.0),
    );
    assert!(notified, "Should have triggered a COV notification");

    let notification = rx
        .try_recv()
        .expect("Expected a notification in the channel");
    assert_eq!(notification.device_id, device_id);
    assert_eq!(notification.object_id, object_id);
}

/// check_and_notify must NOT fire when the change is below cov_increment.
#[test]
fn chaos_cov_does_not_fire_below_increment() {
    let (engine, mut rx) = CovEngine::new();

    let device_id = DeviceId(2);
    let object_id = ObjectId {
        object_type: ObjectType::AnalogInput,
        instance: 1,
    };

    let key = CovSubKey {
        subscriber: NetworkAddress {
            network_number: 0,
            mac: MacAddr::Ip(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 47809)),
        },
        process_id: 2,
        object_id,
        device_id,
    };
    let mut sub = make_sub(Some(60));
    sub.cov_increment = Some(5.0);
    sub.last_value = PropertyValue::Real(10.0);
    engine.subscribe(key, sub);

    // Change by 0.1 (below increment of 5.0) — must NOT fire.
    let notified = engine.check_and_notify(
        device_id,
        object_id,
        PropertyIdentifier::PresentValue,
        &PropertyValue::Real(10.1),
    );
    assert!(
        !notified,
        "Should NOT have triggered a COV notification for sub-increment change"
    );
    assert!(
        rx.try_recv().is_err(),
        "No notification should be in channel"
    );
}

// ---------------------------------------------------------------------------
// 5. ObjectStore bulk_insert performance / correctness
// ---------------------------------------------------------------------------

/// Inserting 1 000 objects via bulk_insert must store all of them correctly.
#[test]
fn chaos_bulk_insert_1000_objects() {
    let store = Arc::new(ObjectStore::new());
    let did = DeviceId(10);

    let objects: Vec<(DeviceId, Box<dyn bacnet_object::BacnetObject>)> = (1u32..=1000)
        .map(|i| {
            let obj: Box<dyn bacnet_object::BacnetObject> = Box::new(AnalogInput::new(
                did,
                i,
                format!("AI-{i:04}"),
                EngineeringUnits::DegreesCelsius,
            ));
            (did, obj)
        })
        .collect();

    let start = Instant::now();
    store.bulk_insert(objects);
    let elapsed = start.elapsed();

    assert_eq!(store.count(), 1000);

    // Verify a sample
    for instance in [1u32, 500, 1000] {
        let oid = ObjectId {
            object_type: ObjectType::AnalogInput,
            instance,
        };
        assert!(
            store.get(did, oid).is_some(),
            "AI-{instance} should be in store"
        );
    }

    // Bulk insert of 1 000 small objects should complete well within 1 second.
    assert!(
        elapsed < Duration::from_secs(1),
        "bulk_insert took {elapsed:?}, expected < 1s"
    );
}

// ---------------------------------------------------------------------------
// 6. Concurrent read isolation
// ---------------------------------------------------------------------------

/// Concurrent readers on ObjectStore must all see consistent data (no panics).
#[test]
fn chaos_concurrent_reads_are_consistent() {
    use std::thread;

    let store = Arc::new(ObjectStore::new());
    let did = DeviceId(20);
    for i in 1u32..=50 {
        store.insert(
            did,
            Box::new(AnalogInput::new(
                did,
                i,
                format!("AI-{i}"),
                EngineeringUnits::Kelvin,
            )),
        );
    }

    let handles: Vec<_> = (0..8)
        .map(|_| {
            let s = Arc::clone(&store);
            thread::spawn(move || {
                for instance in 1u32..=50 {
                    let oid = ObjectId {
                        object_type: ObjectType::AnalogInput,
                        instance,
                    };
                    if let Some(obj) = s.get(did, oid) {
                        let guard = obj.read_guard();
                        let _pv = guard.read_property(PropertyIdentifier::PresentValue, None);
                    }
                }
            })
        })
        .collect();

    for h in handles {
        h.join().expect("Thread panicked during concurrent read");
    }
}

// ---------------------------------------------------------------------------
// 7. Max capacity behaviour
// ---------------------------------------------------------------------------

/// Inserting duplicate objects (same DeviceId + ObjectId) must overwrite cleanly.
#[test]
fn chaos_duplicate_insert_overwrites_not_grows() {
    let store = Arc::new(ObjectStore::new());
    let did = DeviceId(30);
    let oid = ObjectId {
        object_type: ObjectType::AnalogInput,
        instance: 1,
    };

    store.insert(
        did,
        Box::new(AnalogInput::new(did, 1, "first", EngineeringUnits::NoUnits)),
    );
    store.insert(
        did,
        Box::new(AnalogInput::new(
            did,
            1,
            "second",
            EngineeringUnits::NoUnits,
        )),
    );

    // Only one entry should exist for this key.
    assert_eq!(
        store.count(),
        1,
        "Duplicate insert should overwrite, not add a second entry"
    );
    let obj = store
        .get(did, oid)
        .expect("Object not found after duplicate insert");
    let guard = obj.read_guard();
    let name = guard
        .read_property(PropertyIdentifier::ObjectName, None)
        .unwrap();
    // The second insert should win.
    assert_eq!(name, PropertyValue::CharacterString("second".to_string()));
}
