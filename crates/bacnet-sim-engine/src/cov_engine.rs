/// COV (Change of Value) subscription manager.

use bacnet_types::{DeviceId, NetworkAddress, ObjectId, PropertyIdentifier, PropertyValue};
use dashmap::DashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CovSubKey {
    pub subscriber: NetworkAddress,
    pub process_id: u32,
    pub object_id: ObjectId,
    pub device_id: DeviceId,
}

pub struct CovSubscription {
    pub confirmed: bool,
    pub lifetime_secs: Option<u32>,
    pub subscribed_at: Instant,
    pub cov_increment: Option<f32>,
    pub monitored_prop: Option<PropertyIdentifier>,
    pub last_notified_at: Instant,
    pub last_value: PropertyValue,
}

/// A pending COV notification ready to be sent to a subscriber.
#[derive(Debug, Clone)]
pub struct CovNotification {
    pub subscriber: NetworkAddress,
    pub process_id: u32,
    pub device_id: DeviceId,
    pub object_id: ObjectId,
    pub changed_properties: Vec<(PropertyIdentifier, PropertyValue)>,
    pub confirmed: bool,
}

pub struct CovEngine {
    subscriptions: Arc<DashMap<CovSubKey, CovSubscription>>,
    notify_tx: mpsc::UnboundedSender<CovNotification>,
}

impl CovEngine {
    /// Creates a new engine. Returns `(engine, notification_receiver)`.
    pub fn new() -> (Self, mpsc::UnboundedReceiver<CovNotification>) {
        let (notify_tx, notify_rx) = mpsc::unbounded_channel();
        let engine = Self {
            subscriptions: Arc::new(DashMap::new()),
            notify_tx,
        };
        (engine, notify_rx)
    }

    pub fn subscribe(&self, key: CovSubKey, sub: CovSubscription) {
        self.subscriptions.insert(key, sub);
    }

    pub fn unsubscribe(&self, key: &CovSubKey) {
        self.subscriptions.remove(key);
    }

    pub fn active_count(&self) -> usize {
        self.subscriptions.len()
    }

    /// Check if a changed property value triggers any active subscriptions and
    /// enqueue CovNotifications for each matching subscriber.
    pub fn check_and_notify(
        &self,
        device_id: DeviceId,
        object_id: ObjectId,
        property_id: PropertyIdentifier,
        new_value: &PropertyValue,
    ) {
        let now = Instant::now();
        for mut entry in self.subscriptions.iter_mut() {
            // Clone key fields before any mutable access to the value.
            let (sub_addr, sub_pid, key_dev, key_oid) = {
                let k = entry.key();
                (k.subscriber, k.process_id, k.device_id, k.object_id)
            };
            if key_dev != device_id || key_oid != object_id {
                continue;
            }
            if let Some(mp) = entry.monitored_prop {
                if mp != property_id {
                    continue;
                }
            }
            let should_notify = match (new_value, &entry.last_value) {
                (PropertyValue::Real(new), PropertyValue::Real(old)) => {
                    let incr = entry.cov_increment.unwrap_or(0.0);
                    (new - old).abs() >= incr
                }
                _ => new_value != &entry.last_value,
            };
            if should_notify {
                let confirmed = entry.confirmed;
                entry.last_value = new_value.clone();
                entry.last_notified_at = now;
                let notif = CovNotification {
                    subscriber: sub_addr,
                    process_id: sub_pid,
                    device_id,
                    object_id,
                    changed_properties: vec![(property_id, new_value.clone())],
                    confirmed,
                };
                let _ = self.notify_tx.send(notif);
            }
        }
    }

    /// Remove expired subscriptions; returns the count removed.
    pub fn expire_old(&self) -> usize {
        let now = Instant::now();
        let mut removed = 0;
        self.subscriptions.retain(|_, sub| {
            if let Some(lifetime) = sub.lifetime_secs {
                let alive = now.duration_since(sub.subscribed_at).as_secs() < lifetime as u64;
                if !alive {
                    removed += 1;
                }
                alive
            } else {
                true
            }
        });
        removed
    }
}
