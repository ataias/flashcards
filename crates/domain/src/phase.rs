use std::fmt;

use chrono::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    New,
    Learning,
    Review,
    Relearning,
}

impl fmt::Display for Phase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Phase::New => "New",
            Phase::Learning => "Learning",
            Phase::Review => "Review",
            Phase::Relearning => "Relearning",
        })
    }
}

pub const LEARNING_STEPS: [Duration; 2] = [Duration::minutes(1), Duration::minutes(10)];
pub const RELEARNING_STEPS: [Duration; 1] = [Duration::minutes(10)];
