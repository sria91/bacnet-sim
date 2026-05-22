/// Simulation tick engine: drives value models, COV, and alarm engines.

use bacnet_object::store::ObjectStore;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::interval;
use tracing::info;

pub struct SimEngine {
    pub store: Arc<ObjectStore>,
    pub tick_hz: f64,
}

impl SimEngine {
    pub fn new(store: Arc<ObjectStore>, tick_hz: f64) -> Self {
        Self { store, tick_hz }
    }

    pub async fn run(self) {
        let period = Duration::from_secs_f64(1.0 / self.tick_hz);
        let mut ticker = interval(period);
        let start = Instant::now();

        loop {
            ticker.tick().await;
            let t = start.elapsed().as_secs_f64();
            let tick_start = Instant::now();

            // Parallel tick across all shards using rayon
            let store = &self.store;
            rayon::scope(|_s| {
                for shard in store.shards() {
                    let mut guard = shard.write();
                    let now = std::time::SystemTime::now();
                    for obj in guard.values_mut() {
                        obj.tick(now, period);
                    }
                }
            });

            let elapsed = tick_start.elapsed();
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
