/// COV (Change of Value) subscription manager.

use bacnet_types::{DeviceId, ObjectId, PropertyIdentifier, PropertyValue};
use dashmap::DashMap;
use std::sync::Arc;
use std::time::Instant;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CovSubKey {
    pub subscriber: bacnet_types::NetworkAddress,
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

pub struct CovEngine {
    subscriptions: Arc<DashMap<CovSubKey, CovSubscription>>,
}

impl CovEngine {
    pub fn new() -> Self {
        Self { subscriptions: Arc::new(DashMap::new()) }
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

    /// Remove expired subscriptions; return count removed.
    pub fn expire_old(&self) -> usize {
        let now = Instant::now();
        let mut removed = 0;
        self.subscriptions.retain(|_, sub| {
            if let Some(lifetime) = sub.lifetime_secs {
                let still_valid = now.duration_since(sub.subscribed_at).as_secs() < lifetime as u64;
                if !still_valid { removed += 1; }
                still_valid
            } else {
                true
            }
        });
        removed
    }
}

impl Default for CovEngine {
    fn default() -> Self {
        Self::new()
    }
}
