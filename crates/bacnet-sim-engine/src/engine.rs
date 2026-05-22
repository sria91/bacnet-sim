use bacnet_object::store::ObjectStore;
/// Simulation tick engine: drives value models, COV, and alarm engines.
use bacnet_types::{DeviceId, ObjectId, PropertyIdentifier, PropertyValue};
use dashmap::DashMap;
use rand::{rngs::SmallRng, SeedableRng};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::mpsc;
use tokio::time::interval;

use crate::alarm_engine::{AlarmStateMachine, EventNotification};
use crate::cov_engine::{CovEngine, CovNotification};
use crate::value_model::ValueModel;

pub struct ModelEntry {
    pub model: Box<dyn ValueModel>,
    pub rng: SmallRng,
    /// Minimum delta before a COV notification fires.
    pub cov_increment: f32,
    /// Last value successfully applied (used for COV delta check).
    pub last_value: f32,
    /// Optional intrinsic alarm state machine for this object.
    pub alarm: Option<AlarmStateMachine>,
}

pub struct SimEngine {
    pub store: Arc<ObjectStore>,
    pub tick_hz: f64,
    /// Map from `(device_instance_key, object_key)` → model entry.
    /// The key format mirrors `ObjectStore`'s internal shard key layout.
    models: DashMap<(u64, u64), ModelEntry>,
    pub cov_engine: Arc<CovEngine>,
    /// Last tick duration in nanoseconds. Read by external metrics tasks.
    pub last_tick_nanos: Arc<AtomicU64>,
    /// Cumulative COV notifications dispatched.
    pub cov_notifications_total: Arc<AtomicU64>,
}

impl SimEngine {
    /// Create a minimal engine with no value models.
    pub fn new(
        store: Arc<ObjectStore>,
        tick_hz: f64,
    ) -> (Self, mpsc::UnboundedReceiver<CovNotification>) {
        let (cov_engine, cov_rx) = CovEngine::new();
        let engine = Self {
            store,
            tick_hz,
            models: DashMap::new(),
            cov_engine: Arc::new(cov_engine),
            last_tick_nanos: Arc::new(AtomicU64::new(0)),
            cov_notifications_total: Arc::new(AtomicU64::new(0)),
        };
        (engine, cov_rx)
    }

    /// Register a value model for a specific (device, object) pair.
    pub fn add_model(
        &self,
        device_id: DeviceId,
        object_id: ObjectId,
        model: Box<dyn ValueModel>,
        cov_increment: f32,
        alarm: Option<AlarmStateMachine>,
        seed: u64,
    ) {
        let (dk, ok) = obj_key(device_id, object_id);
        self.models.insert(
            (dk, ok),
            ModelEntry {
                model,
                rng: SmallRng::seed_from_u64(seed),
                cov_increment: cov_increment.max(0.0),
                last_value: 0.0,
                alarm,
            },
        );
    }

    /// Run the tick loop indefinitely.
    pub async fn run(self) {
        let period = Duration::from_secs_f64(1.0 / self.tick_hz);
        let mut ticker = interval(period);
        let start = Instant::now();
        let mut last_expire = Instant::now();

        loop {
            ticker.tick().await;
            let t = start.elapsed().as_secs_f64();
            let now_sys = SystemTime::now();
            let now = Instant::now();
            let tick_start = now;

            // Advance all value models and collect changed (device, object, value) triples.
            let mut changed: Vec<(DeviceId, ObjectId, f32)> = Vec::new();

            for shard in self.store.shards() {
                let mut guard = shard.write();
                for ((dk, ok), obj) in guard.iter_mut() {
                    if let Some(mut entry) = self.models.get_mut(&(*dk, *ok)) {
                        // Destructure to split the mutable borrow across fields.
                        let ModelEntry {
                            ref mut model,
                            ref mut rng,
                            ref cov_increment,
                            ref mut last_value,
                            ref mut alarm,
                        } = *entry;
                        let new_val = model.next(t, rng);
                        let _ = obj.force_write_property(
                            PropertyIdentifier::PresentValue,
                            PropertyValue::Real(new_val),
                        );
                        obj.tick(now_sys, period);

                        // Evaluate alarm state machine
                        if let Some(ref mut sm) = alarm {
                            let pv = PropertyValue::Real(new_val);
                            if let Some(notif) = sm.evaluate(&pv, now, obj.object_id()) {
                                tracing::info!(
                                    device = obj.device_id().0,
                                    object = ?obj.object_id(),
                                    from = ?notif.from_state,
                                    to = ?notif.to_state,
                                    "Alarm transition"
                                );
                            }
                        }

                        let delta = (new_val - *last_value).abs();
                        if delta >= *cov_increment {
                            *last_value = new_val;
                            changed.push((obj.device_id(), obj.object_id(), new_val));
                        }
                    } else {
                        obj.tick(now_sys, period);
                    }
                }
            }

            // Process COV notifications outside the shard write locks.
            let mut cov_sent = 0u64;
            for (dev, oid, val) in changed {
                if self.cov_engine.check_and_notify(
                    dev,
                    oid,
                    PropertyIdentifier::PresentValue,
                    &PropertyValue::Real(val),
                ) {
                    cov_sent += 1;
                }
            }
            if cov_sent > 0 {
                self.cov_notifications_total
                    .fetch_add(cov_sent, Ordering::Relaxed);
            }

            // Periodically expire stale COV subscriptions.
            if now.duration_since(last_expire).as_secs() >= 10 {
                let removed = self.cov_engine.expire_old();
                if removed > 0 {
                    tracing::debug!("Expired {removed} COV subscriptions");
                }
                last_expire = now;
            }

            let elapsed = tick_start.elapsed();
            self.last_tick_nanos
                .store(elapsed.as_nanos() as u64, Ordering::Relaxed);
            if elapsed > period {
                tracing::warn!(
                    "Tick loop behind schedule: took {:.1}ms, budget {:.1}ms",
                    elapsed.as_secs_f64() * 1000.0,
                    period.as_secs_f64() * 1000.0,
                );
            }
        }
    }
}

/// Compute the `(dk, ok)` key that mirrors ObjectStore's internal shard key.
fn obj_key(device: DeviceId, obj: ObjectId) -> (u64, u64) {
    let dk = device.0 as u64;
    let ok = ((obj.object_type as u64) << 22) | obj.instance as u64;
    (dk, ok)
}

// ── Unit tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value_model::ConstantModel;
    use bacnet_types::{DeviceId, ObjectId, ObjectType};

    #[test]
    fn obj_key_deterministic() {
        let dev = DeviceId(42);
        let oid = ObjectId {
            object_type: ObjectType::AnalogInput,
            instance: 7,
        };
        let (dk, ok) = obj_key(dev, oid);
        assert_eq!(dk, 42);
        // AnalogInput = 0, so ok = (0 << 22) | 7 = 7
        assert_eq!(ok, 7);
    }

    #[tokio::test]
    async fn add_model_registers_entry() {
        let store = Arc::new(ObjectStore::new());
        let (engine, _rx) = SimEngine::new(store, 1.0);
        let dev = DeviceId(1);
        let oid = ObjectId {
            object_type: ObjectType::AnalogInput,
            instance: 1,
        };
        engine.add_model(dev, oid, Box::new(ConstantModel(42.0)), 0.1, None, 0);
        assert_eq!(engine.models.len(), 1);
    }
}
