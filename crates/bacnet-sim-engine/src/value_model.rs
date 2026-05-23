/// Value models — drive `present_value` changes over time.
use rand::{rngs::SmallRng, RngExt};

pub trait ValueModel: Send + Sync {
    /// Return the next value given elapsed simulation time `t` (seconds) and `rng`.
    fn next(&mut self, t: f64, rng: &mut SmallRng) -> f32;
}

// ---------------------------------------------------------------------------
// Constant
// ---------------------------------------------------------------------------

pub struct ConstantModel(pub f32);

impl ValueModel for ConstantModel {
    fn next(&mut self, _t: f64, _rng: &mut SmallRng) -> f32 {
        self.0
    }
}

// ---------------------------------------------------------------------------
// Sine wave with optional noise
// ---------------------------------------------------------------------------

pub struct SineModel {
    pub amplitude: f32,
    pub period_s: f64,
    pub offset: f32,
    pub noise_std: f32,
}

impl ValueModel for SineModel {
    fn next(&mut self, t: f64, rng: &mut SmallRng) -> f32 {
        let base = self.offset
            + self.amplitude * (2.0 * std::f64::consts::PI * t / self.period_s).sin() as f32;
        if self.noise_std > 0.0 {
            base + rng.random::<f32>() * self.noise_std * 2.0 - self.noise_std
        } else {
            base
        }
    }
}

// ---------------------------------------------------------------------------
// Random walk (Brownian motion)
// ---------------------------------------------------------------------------

pub struct RandomWalkModel {
    pub current: f32,
    pub step_std: f32,
    pub min: f32,
    pub max: f32,
}

impl ValueModel for RandomWalkModel {
    fn next(&mut self, _t: f64, rng: &mut SmallRng) -> f32 {
        let delta = (rng.random::<f32>() - 0.5) * 2.0 * self.step_std;
        self.current = (self.current + delta).clamp(self.min, self.max);
        self.current
    }
}

// ---------------------------------------------------------------------------
// Step function (schedule-driven)
// ---------------------------------------------------------------------------

pub struct StepModel {
    pub schedule: Vec<(f64, f32)>, // (time_s, value)
}

impl ValueModel for StepModel {
    fn next(&mut self, t: f64, _rng: &mut SmallRng) -> f32 {
        let mut current = self.schedule.first().map(|e| e.1).unwrap_or(0.0);
        for &(ts, val) in &self.schedule {
            if t >= ts {
                current = val;
            }
        }
        current
    }
}

// ---------------------------------------------------------------------------
// Thermal / exponential approach
// ---------------------------------------------------------------------------

pub struct ThermalModel {
    pub setpoint: f32,
    pub current: f32,
    pub time_const_s: f64,
    pub ambient: f32,
    pub noise_std: f32,
    pub last_t: f64,
}

impl ValueModel for ThermalModel {
    fn next(&mut self, t: f64, rng: &mut SmallRng) -> f32 {
        let dt = (t - self.last_t).max(0.0);
        self.last_t = t;
        let alpha = 1.0 - (-dt / self.time_const_s).exp() as f32;
        self.current += alpha * (self.setpoint - self.current);
        let noise = if self.noise_std > 0.0 {
            (rng.random::<f32>() - 0.5) * 2.0 * self.noise_std
        } else {
            0.0
        };
        self.current + noise
    }
}

// ---------------------------------------------------------------------------
// Composite (sum)
// ---------------------------------------------------------------------------

pub struct CompositeModel(pub Vec<Box<dyn ValueModel>>);

impl ValueModel for CompositeModel {
    fn next(&mut self, t: f64, rng: &mut SmallRng) -> f32 {
        self.0.iter_mut().map(|m| m.next(t, rng)).sum()
    }
}
