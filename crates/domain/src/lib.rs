//! Card, Deck, Review, and FSRS orchestration.

mod queue;
mod rating;
mod schedule;

use chrono::{DateTime, Utc};

pub use fsrs::MemoryState;
pub use queue::{
    NEW_CARDS_PER_LOCAL_DAY, apply_daily_new_cap, capped_new_count_for_local_day, is_due_at,
    new_cards_introduced_on_local_day, remaining_new_card_slots, select_study_queue,
};
pub use rating::Rating;
pub use schedule::{DESIRED_RETENTION, ScheduleError, ScheduledReview, schedule};

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
