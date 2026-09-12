use chrono::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    New,
    Learning,
    Review,
    Relearning,
}

pub const LEARNING_STEPS: [Duration; 2] = [Duration::minutes(1), Duration::minutes(10)];
pub const RELEARNING_STEPS: [Duration; 1] = [Duration::minutes(10)];
