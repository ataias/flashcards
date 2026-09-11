//! Card, Deck, Review, and FSRS orchestration.

mod queue;
mod rating;
mod schedule;

use chrono::{DateTime, Utc};

pub use fsrs::MemoryState;
pub use queue::{new_cards_introduced_on_local_day, select_study_queue, NEW_CARDS_PER_LOCAL_DAY};
pub use rating::Rating;
pub use schedule::{schedule, ScheduleError, ScheduledReview, DESIRED_RETENTION};

pub type CardId = i64;

/// A Card: front/back plus optional FSRS memory state and due time.
///
/// `memory` is `None` for a New Card (never Reviewed).
#[derive(Debug, Clone, PartialEq)]
pub struct Card {
    pub id: CardId,
    pub deck_id: i64,
    pub front: String,
    pub back: String,
    pub memory: Option<MemoryState>,
    pub due: Option<DateTime<Utc>>,
    pub last_review: Option<DateTime<Utc>>,
}

impl Card {
    pub fn is_new(&self) -> bool {
        self.memory.is_none()
    }
}
