/// Sharded object store for millions of BACnet objects.

use bacnet_types::{DeviceId, ObjectId, error::BacnetError};
use parking_lot::RwLock;
use rustc_hash::FxHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use crate::property::BacnetObject;

const NUM_SHARDS: usize = 256;

type Shard = RwLock<HashMap<(u64, u64), Box<dyn BacnetObject>>>;

pub struct ObjectStore {
    shards: Vec<Arc<Shard>>,
}

/// A locked reference to an object that supports `read()` and `write()`.
pub struct ObjectRef(Arc<Shard>, u64, u64);

impl ObjectRef {
    pub fn read(&self) -> parking_lot::RwLockReadGuard<'_, HashMap<(u64, u64), Box<dyn BacnetObject>>> {
        self.0.read()
    }
    pub fn write(&self) -> parking_lot::RwLockWriteGuard<'_, HashMap<(u64, u64), Box<dyn BacnetObject>>> {
        self.0.write()
    }
}

// Convenience: make it easy to call obj methods through the guard
pub struct ObjectReadGuard<'a>(parking_lot::RwLockReadGuard<'a, HashMap<(u64, u64), Box<dyn BacnetObject>>>, (u64, u64));

impl<'a> ObjectReadGuard<'a> {
    pub fn read_property(
        &self,
        property_id: bacnet_types::PropertyIdentifier,
        array_index: Option<u32>,
    ) -> Result<bacnet_types::PropertyValue, BacnetError> {
        self.0[&self.1].read_property(property_id, array_index)
    }

    pub fn all_properties(&self) -> Vec<(bacnet_types::PropertyIdentifier, bacnet_types::PropertyValue)> {
        self.0[&self.1].all_properties()
    }
}

pub struct ObjectWriteGuard<'a>(parking_lot::RwLockWriteGuard<'a, HashMap<(u64, u64), Box<dyn BacnetObject>>>, (u64, u64));

impl<'a> ObjectWriteGuard<'a> {
    pub fn write_property(
        &mut self,
        property_id: bacnet_types::PropertyIdentifier,
        array_index: Option<u32>,
        value: bacnet_types::PropertyValue,
        priority: Option<u8>,
    ) -> Result<(), BacnetError> {
        self.0.get_mut(&self.1).unwrap().write_property(property_id, array_index, value, priority)
    }

    /// Simulation-internal write, bypasses out-of-service guards.
    pub fn force_write_property(
        &mut self,
        property_id: bacnet_types::PropertyIdentifier,
        value: bacnet_types::PropertyValue,
    ) -> Result<(), BacnetError> {
        self.0.get_mut(&self.1).unwrap().force_write_property(property_id, value)
    }
}

fn shard_key(device: DeviceId, obj: ObjectId) -> (usize, u64, u64) {
    let dk = device.0 as u64;
    let ok = ((obj.object_type as u64) << 22) | obj.instance as u64;
    let mut h = FxHasher::default();
    dk.hash(&mut h);
    ok.hash(&mut h);
    let shard = (h.finish() as usize) % NUM_SHARDS;
    (shard, dk, ok)
}

impl ObjectStore {
    pub fn new() -> Self {
        let shards = (0..NUM_SHARDS)
            .map(|_| Arc::new(RwLock::new(HashMap::new())))
            .collect();
        Self { shards }
    }

    pub fn insert(&self, device: DeviceId, obj: Box<dyn BacnetObject>) {
        let oid = obj.object_id();
        let (shard_idx, dk, ok) = shard_key(device, oid);
        self.shards[shard_idx].write().insert((dk, ok), obj);
    }

    /// Returns `Some(shard_arc)` that can be locked to access the object.
    pub fn get(&self, device: DeviceId, obj_id: ObjectId) -> Option<ObjectRef> {
        let (shard_idx, dk, ok) = shard_key(device, obj_id);
        let shard = self.shards[shard_idx].clone();
        if shard.read().contains_key(&(dk, ok)) {
            Some(ObjectRef(shard, dk, ok))
        } else {
            None
        }
    }

    pub fn shards(&self) -> &[Arc<Shard>] {
        &self.shards
    }
}

impl Default for ObjectStore {
    fn default() -> Self {
        Self::new()
    }
}

// Make ObjectRef actually useful for reading/writing
impl ObjectRef {
    pub fn read_guard(&self) -> ObjectReadGuard<'_> {
        ObjectReadGuard(self.0.read(), (self.1, self.2))
    }
    pub fn write_guard(&mut self) -> ObjectWriteGuard<'_> {
        ObjectWriteGuard(self.0.write(), (self.1, self.2))
    }

    /// Convenience: write a single property without needing a separate guard.
    pub fn write_property_once(
        mut self,
        property_id: bacnet_types::PropertyIdentifier,
        array_index: Option<u32>,
        value: bacnet_types::PropertyValue,
        priority: Option<u8>,
    ) -> Result<(), BacnetError> {
        self.write_guard().write_property(property_id, array_index, value, priority)
    }

    /// Simulation-internal write; bypasses out-of-service guards.
    pub fn force_write_property_once(
        mut self,
        property_id: bacnet_types::PropertyIdentifier,
        value: bacnet_types::PropertyValue,
    ) -> Result<(), BacnetError> {
        self.write_guard().force_write_property(property_id, value)
    }
}

