/// Alarm / intrinsic reporting state machine (ASHRAE 135-2020 §13).

use bacnet_types::{ObjectId, PropertyValue};
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EventState {
    #[default]
    Normal = 0,
    Fault = 1,
    OffNormal = 2,
    HighLimit = 3,
    LowLimit = 4,
    LifeSafetyAlarm = 5,
}

pub enum IntrinsicAlgorithm {
    OutOfRange { high_limit: f32, low_limit: f32, deadband: f32 },
    ChangeOfValue { cov_increment: f32 },
    CommandFailure { feedback_timeout: std::time::Duration },
}

pub struct EventNotification {
    pub event_object: ObjectId,
    pub from_state: EventState,
    pub to_state: EventState,
    pub timestamp: Instant,
}

pub struct AlarmStateMachine {
    pub current_state: EventState,
    pub time_delay: std::time::Duration,
    pub notification_class: u32,
    pub algorithm: IntrinsicAlgorithm,
    pub transition_start: Option<Instant>,
}

impl AlarmStateMachine {
    pub fn evaluate(
        &mut self,
        value: &PropertyValue,
        now: Instant,
        object_id: ObjectId,
    ) -> Option<EventNotification> {
        match &self.algorithm {
            IntrinsicAlgorithm::OutOfRange { high_limit, low_limit, deadband } => {
                let v = value.as_f32()?;
                let new_state = if v > *high_limit {
                    EventState::HighLimit
                } else if v < *low_limit {
                    EventState::LowLimit
                } else {
                    // Apply deadband when returning to normal
                    let in_dead = match self.current_state {
                        EventState::HighLimit => v > *high_limit - *deadband,
                        EventState::LowLimit  => v < *low_limit  + *deadband,
                        _ => false,
                    };
                    if in_dead { self.current_state } else { EventState::Normal }
                };

                if new_state == self.current_state {
                    self.transition_start = None;
                    return None;
                }

                // Time-delay gate
                let start = self.transition_start.get_or_insert(now);
                if now.duration_since(*start) < self.time_delay {
                    return None;
                }

                let from = self.current_state;
                self.current_state = new_state;
                self.transition_start = None;
                Some(EventNotification {
                    event_object: object_id,
                    from_state: from,
                    to_state: new_state,
                    timestamp: now,
                })
            }
            _ => None,
        }
    }
}
