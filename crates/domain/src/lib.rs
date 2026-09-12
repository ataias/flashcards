//! Card, Deck, Review, Store, and study/home use cases.

mod error;
mod home;
mod queue;
mod rating;
mod schedule;
mod store;
mod use_cases;

use chrono::{DateTime, Utc};

pub use error::Error;
pub use fsrs::MemoryState;
pub use home::summarize_home;
pub use queue::{
    NEW_CARDS_PER_LOCAL_DAY, apply_daily_new_cap, capped_new_count_for_local_day, is_due_at,
    new_cards_introduced_on_local_day, remaining_new_card_slots, select_study_queue,
};
pub use rating::Rating;
pub use schedule::{DESIRED_RETENTION, ScheduleError, ScheduledReview, schedule};
pub use store::{HomeDeckInput, HomeInputs, Store, StudyInputs};
pub use use_cases::{
    create_card, create_deck, delete_card, delete_deck, get_card, get_deck, list_deck_cards,
    list_home, next_study_card, rate, rename_deck, update_card,
};

pub type CardId = i64;
pub type DeckId = i64;

/// A Card: front/back plus optional FSRS memory state and due time.
///
/// `memory` is `None` for a New Card (never Reviewed).
#[derive(Debug, Clone, PartialEq)]
pub struct Card {
    pub id: CardId,
    pub deck_id: DeckId,
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

    pub fn apply_rating(
        &self,
        rating: Rating,
        now: DateTime<Utc>,
    ) -> Result<(Card, ReviewLogEntry), ScheduleError> {
        let scheduled = schedule(self.memory, self.last_review, rating, now)?;
        let mut card = self.clone();
        card.memory = Some(scheduled.memory);
        card.due = Some(scheduled.due);
        card.last_review = Some(scheduled.last_review);
        let entry = ReviewLogEntry {
            card_id: self.id,
            rated_at: scheduled.last_review,
            rating,
        };
        Ok((card, entry))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deck {
    pub id: DeckId,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeckSummary {
    pub deck: Deck,
    pub due_count: usize,
    pub new_count: usize,
}

/// Front/back list row for a Deck page (no FSRS columns).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardText {
    pub id: CardId,
    pub front: String,
    pub back: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewLogEntry {
    pub card_id: CardId,
    pub rated_at: DateTime<Utc>,
    pub rating: Rating,
}

#[cfg(test)]
mod apply_rating_tests {
    use super::*;
    use chrono::{Duration, TimeZone};

    fn noon() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 11, 12, 0, 0).unwrap()
    }

    fn new_card() -> Card {
        Card {
            id: 7,
            deck_id: 1,
            front: "front".into(),
            back: "back".into(),
            memory: None,
            due: None,
            last_review: None,
        }
    }

    #[test]
    fn good_on_new_card_matches_schedule_and_writes_log() {
        let card = new_card();
        let now = noon();
        let (updated, entry) = card.apply_rating(Rating::Good, now).unwrap();
        let scheduled = schedule(None, None, Rating::Good, now).unwrap();

        assert_eq!(updated.id, card.id);
        assert_eq!(updated.deck_id, card.deck_id);
        assert_eq!(updated.front, card.front);
        assert_eq!(updated.back, card.back);
        assert_eq!(updated.memory, Some(scheduled.memory));
        assert_eq!(updated.due, Some(scheduled.due));
        assert_eq!(updated.last_review, Some(now));
        assert!(!updated.is_new());

        assert_eq!(entry.card_id, card.id);
        assert_eq!(entry.rated_at, now);
        assert_eq!(entry.rating, Rating::Good);

        assert!(card.is_new());
        assert_eq!(card.memory, None);
        assert_eq!(card.due, None);
        assert_eq!(card.last_review, None);
    }

    #[test]
    fn again_interval_is_at_least_one_day() {
        let now = noon();
        let (updated, entry) = new_card().apply_rating(Rating::Again, now).unwrap();
        assert!(updated.due.unwrap() >= now + Duration::days(1));
        assert_eq!(entry.rating, Rating::Again);
        assert_eq!(entry.rated_at, now);

        let reviewed = Card {
            id: 3,
            deck_id: 1,
            front: "front".into(),
            back: "back".into(),
            memory: Some(MemoryState {
                stability: 0.1,
                difficulty: 10.0,
            }),
            due: Some(now),
            last_review: Some(now - Duration::days(1)),
        };
        let (updated, _) = reviewed.apply_rating(Rating::Again, now).unwrap();
        assert!(updated.due.unwrap() >= now + Duration::days(1));
    }

    #[test]
    fn reviewed_card_uses_elapsed_days_from_last_review() {
        let last = Utc.with_ymd_and_hms(2026, 9, 1, 12, 0, 0).unwrap();
        let now = noon();
        let memory = MemoryState {
            stability: 5.0,
            difficulty: 5.0,
        };
        let card = Card {
            id: 4,
            deck_id: 2,
            front: "q".into(),
            back: "a".into(),
            memory: Some(memory),
            due: Some(now),
            last_review: Some(last),
        };
        let (updated, entry) = card.apply_rating(Rating::Good, now).unwrap();
        let scheduled = schedule(Some(memory), Some(last), Rating::Good, now).unwrap();
        assert_eq!(updated.memory, Some(scheduled.memory));
        assert_eq!(updated.due, Some(scheduled.due));
        assert_eq!(updated.last_review, Some(now));
        assert_eq!(entry.card_id, 4);
        assert_eq!(entry.rating, Rating::Good);
    }
}
