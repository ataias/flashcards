//! Card, Deck, Review, Store, and study/home use cases.

mod error;
mod home;
mod phase;
mod queue;
mod rating;
mod schedule;
mod store;
mod use_cases;

use chrono::{DateTime, Utc};

pub use error::Error;
pub use fsrs::MemoryState;
pub use home::summarize_home;
pub use phase::{LEARNING_STEPS, Phase, RELEARNING_STEPS};
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

#[derive(Debug, Clone, PartialEq)]
pub struct Card {
    pub id: CardId,
    pub deck_id: DeckId,
    pub front: String,
    pub back: String,
    pub phase: Phase,
    pub learning_step: Option<usize>,
    pub memory: Option<MemoryState>,
    pub due: Option<DateTime<Utc>>,
    pub last_review: Option<DateTime<Utc>>,
}

impl Card {
    pub fn is_new(&self) -> bool {
        self.phase == Phase::New
    }

    pub fn apply_rating(
        &self,
        rating: Rating,
        now: DateTime<Utc>,
    ) -> Result<(Card, ReviewLogEntry), ScheduleError> {
        let mut card = self.clone();
        if card.phase == Phase::New {
            card.phase = Phase::Learning;
            card.learning_step = Some(0);
        }
        match card.phase {
            Phase::Learning | Phase::New => {
                apply_step_rating(&mut card, rating, now, &LEARNING_STEPS, Phase::Learning)?;
            }
            Phase::Relearning => {
                apply_step_rating(&mut card, rating, now, &RELEARNING_STEPS, Phase::Relearning)?;
            }
            Phase::Review => apply_review_rating(&mut card, rating, now)?,
        }
        card.last_review = Some(now);
        Ok((
            card,
            ReviewLogEntry {
                card_id: self.id,
                rated_at: now,
                rating,
            },
        ))
    }
}

fn apply_step_rating(
    card: &mut Card,
    rating: Rating,
    now: DateTime<Utc>,
    steps: &[chrono::Duration],
    stay: Phase,
) -> Result<(), ScheduleError> {
    let last = steps.len().saturating_sub(1);
    let i = card.learning_step.unwrap_or(0).min(last);
    match rating {
        Rating::Again => {
            card.phase = stay;
            card.learning_step = Some(0);
            card.due = Some(now + steps[0]);
        }
        Rating::Hard => {
            card.phase = stay;
            card.learning_step = Some(i);
            card.due = Some(now + steps[i]);
        }
        Rating::Good => {
            let next = i + 1;
            if next < steps.len() {
                card.phase = stay;
                card.learning_step = Some(next);
                card.due = Some(now + steps[next]);
            } else {
                graduate(card, Rating::Good, now)?;
            }
        }
        Rating::Easy => graduate(card, Rating::Easy, now)?,
    }
    Ok(())
}

fn apply_review_rating(
    card: &mut Card,
    rating: Rating,
    now: DateTime<Utc>,
) -> Result<(), ScheduleError> {
    let scheduled = schedule(card.memory, card.last_review, rating, now)?;
    card.memory = Some(scheduled.memory);
    match rating {
        Rating::Again => {
            card.phase = Phase::Relearning;
            card.learning_step = Some(0);
            card.due = Some(now + RELEARNING_STEPS[0]);
        }
        Rating::Hard | Rating::Good | Rating::Easy => {
            card.phase = Phase::Review;
            card.learning_step = None;
            card.due = Some(scheduled.due);
        }
    }
    Ok(())
}

fn graduate(card: &mut Card, rating: Rating, now: DateTime<Utc>) -> Result<(), ScheduleError> {
    let scheduled = schedule(card.memory, card.last_review, rating, now)?;
    card.phase = Phase::Review;
    card.learning_step = None;
    card.memory = Some(scheduled.memory);
    card.due = Some(scheduled.due);
    Ok(())
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
            phase: Phase::New,
            learning_step: None,
            memory: None,
            due: None,
            last_review: None,
        }
    }

    fn learning_card(step: usize, last_review: DateTime<Utc>, due: DateTime<Utc>) -> Card {
        Card {
            id: 8,
            deck_id: 1,
            front: "front".into(),
            back: "back".into(),
            phase: Phase::Learning,
            learning_step: Some(step),
            memory: None,
            due: Some(due),
            last_review: Some(last_review),
        }
    }

    fn review_card(memory: MemoryState, last_review: DateTime<Utc>, due: DateTime<Utc>) -> Card {
        Card {
            id: 4,
            deck_id: 2,
            front: "q".into(),
            back: "a".into(),
            phase: Phase::Review,
            learning_step: None,
            memory: Some(memory),
            due: Some(due),
            last_review: Some(last_review),
        }
    }

    #[test]
    fn new_again_enters_learning_at_first_step() {
        let card = new_card();
        let now = noon();
        let (updated, entry) = card.apply_rating(Rating::Again, now).unwrap();
        assert_eq!(updated.phase, Phase::Learning);
        assert_eq!(updated.learning_step, Some(0));
        assert_eq!(updated.due, Some(now + LEARNING_STEPS[0]));
        assert_eq!(updated.memory, None);
        assert_eq!(updated.last_review, Some(now));
        assert_eq!(entry.card_id, card.id);
        assert_eq!(entry.rated_at, now);
        assert_eq!(entry.rating, Rating::Again);
        assert!(card.is_new());
        assert!(!updated.is_new());
    }

    #[test]
    fn new_hard_repeats_first_learning_step() {
        let now = noon();
        let (updated, _) = new_card().apply_rating(Rating::Hard, now).unwrap();
        assert_eq!(updated.phase, Phase::Learning);
        assert_eq!(updated.learning_step, Some(0));
        assert_eq!(updated.due, Some(now + LEARNING_STEPS[0]));
        assert_eq!(updated.memory, None);
    }

    #[test]
    fn new_good_advances_to_second_learning_step() {
        let card = new_card();
        let now = noon();
        let (updated, entry) = card.apply_rating(Rating::Good, now).unwrap();
        assert_eq!(updated.phase, Phase::Learning);
        assert_eq!(updated.learning_step, Some(1));
        assert_eq!(updated.due, Some(now + LEARNING_STEPS[1]));
        assert_eq!(updated.memory, None);
        assert_eq!(updated.last_review, Some(now));
        assert_eq!(entry.rating, Rating::Good);
        assert_eq!(updated.front, card.front);
        assert_eq!(updated.back, card.back);
    }

    #[test]
    fn first_rating_from_new_matches_learning_rules_at_step_zero() {
        let now = noon();
        let at_step_zero = Card {
            phase: Phase::Learning,
            learning_step: Some(0),
            ..new_card()
        };
        for rating in [Rating::Again, Rating::Hard, Rating::Good, Rating::Easy] {
            let from_new = new_card().apply_rating(rating, now).unwrap().0;
            let from_learning = at_step_zero.apply_rating(rating, now).unwrap().0;
            assert_eq!(from_new.phase, from_learning.phase);
            assert_eq!(from_new.learning_step, from_learning.learning_step);
            assert_eq!(from_new.memory, from_learning.memory);
            assert_eq!(from_new.due, from_learning.due);
            assert_eq!(from_new.last_review, from_learning.last_review);
            assert!(!from_new.is_new());
        }
    }

    #[test]
    fn new_easy_graduates_with_fsrs_easy() {
        let now = noon();
        let (updated, entry) = new_card().apply_rating(Rating::Easy, now).unwrap();
        let scheduled = schedule(None, None, Rating::Easy, now).unwrap();
        assert_eq!(updated.phase, Phase::Review);
        assert_eq!(updated.learning_step, None);
        assert_eq!(updated.memory, Some(scheduled.memory));
        assert_eq!(updated.due, Some(scheduled.due));
        assert_eq!(entry.rating, Rating::Easy);
        assert!(!updated.is_new());
    }

    #[test]
    fn learning_again_restarts_at_first_step() {
        let now = noon();
        let card = learning_card(1, now - Duration::minutes(10), now);
        let (updated, _) = card.apply_rating(Rating::Again, now).unwrap();
        assert_eq!(updated.phase, Phase::Learning);
        assert_eq!(updated.learning_step, Some(0));
        assert_eq!(updated.due, Some(now + LEARNING_STEPS[0]));
        assert_eq!(updated.memory, None);
    }

    #[test]
    fn learning_hard_repeats_current_step() {
        let now = noon();
        let card = learning_card(1, now - Duration::minutes(10), now);
        let (updated, _) = card.apply_rating(Rating::Hard, now).unwrap();
        assert_eq!(updated.phase, Phase::Learning);
        assert_eq!(updated.learning_step, Some(1));
        assert_eq!(updated.due, Some(now + LEARNING_STEPS[1]));
    }

    #[test]
    fn learning_good_on_last_step_graduates() {
        let now = noon();
        let last = now - Duration::minutes(10);
        let card = learning_card(1, last, now);
        let (updated, _) = card.apply_rating(Rating::Good, now).unwrap();
        let scheduled = schedule(None, Some(last), Rating::Good, now).unwrap();
        assert_eq!(updated.phase, Phase::Review);
        assert_eq!(updated.learning_step, None);
        assert_eq!(updated.memory, Some(scheduled.memory));
        assert_eq!(updated.due, Some(scheduled.due));
    }

    #[test]
    fn learning_easy_graduates_early() {
        let now = noon();
        let last = now - Duration::minutes(1);
        let card = learning_card(0, last, now);
        let (updated, _) = card.apply_rating(Rating::Easy, now).unwrap();
        let scheduled = schedule(None, Some(last), Rating::Easy, now).unwrap();
        assert_eq!(updated.phase, Phase::Review);
        assert_eq!(updated.memory, Some(scheduled.memory));
        assert_eq!(updated.due, Some(scheduled.due));
    }

    #[test]
    fn review_again_updates_memory_then_enters_relearning() {
        let now = noon();
        let last = now - Duration::days(1);
        let memory = MemoryState {
            stability: 0.1,
            difficulty: 10.0,
        };
        let card = review_card(memory, last, now);
        let (updated, entry) = card.apply_rating(Rating::Again, now).unwrap();
        let scheduled = schedule(Some(memory), Some(last), Rating::Again, now).unwrap();
        assert_eq!(updated.phase, Phase::Relearning);
        assert_eq!(updated.learning_step, Some(0));
        assert_eq!(updated.memory, Some(scheduled.memory));
        assert_eq!(updated.due, Some(now + RELEARNING_STEPS[0]));
        assert_ne!(updated.due, Some(scheduled.due));
        assert_eq!(entry.rating, Rating::Again);
    }

    #[test]
    fn review_good_uses_elapsed_days_and_unfloored_fsrs() {
        let last = Utc.with_ymd_and_hms(2026, 9, 1, 12, 0, 0).unwrap();
        let now = noon();
        let memory = MemoryState {
            stability: 5.0,
            difficulty: 5.0,
        };
        let card = review_card(memory, last, now);
        let (updated, entry) = card.apply_rating(Rating::Good, now).unwrap();
        let scheduled = schedule(Some(memory), Some(last), Rating::Good, now).unwrap();
        assert_eq!(updated.phase, Phase::Review);
        assert_eq!(updated.learning_step, None);
        assert_eq!(updated.memory, Some(scheduled.memory));
        assert_eq!(updated.due, Some(scheduled.due));
        assert_eq!(updated.last_review, Some(now));
        assert_eq!(entry.card_id, 4);
        assert_eq!(entry.rating, Rating::Good);
    }

    #[test]
    fn review_due_can_be_shorter_than_one_day() {
        let now = noon();
        let last = now - Duration::days(1);
        let memory = MemoryState {
            stability: 0.1,
            difficulty: 10.0,
        };
        let card = review_card(memory, last, now);
        let (updated, _) = card.apply_rating(Rating::Good, now).unwrap();
        assert_eq!(updated.phase, Phase::Review);
        assert!(updated.due.unwrap() < now + Duration::days(1));
        assert!(updated.due.unwrap() > now);
    }

    #[test]
    fn relearning_good_graduates_back_to_review() {
        let now = noon();
        let last = now - Duration::minutes(10);
        let memory = MemoryState {
            stability: 0.5,
            difficulty: 8.0,
        };
        let card = Card {
            id: 9,
            deck_id: 1,
            front: "front".into(),
            back: "back".into(),
            phase: Phase::Relearning,
            learning_step: Some(0),
            memory: Some(memory),
            due: Some(now),
            last_review: Some(last),
        };
        let (updated, _) = card.apply_rating(Rating::Good, now).unwrap();
        let scheduled = schedule(Some(memory), Some(last), Rating::Good, now).unwrap();
        assert_eq!(updated.phase, Phase::Review);
        assert_eq!(updated.learning_step, None);
        assert_eq!(updated.memory, Some(scheduled.memory));
        assert_eq!(updated.due, Some(scheduled.due));
    }

    #[test]
    fn relearning_again_restarts_single_step() {
        let now = noon();
        let memory = MemoryState {
            stability: 0.5,
            difficulty: 8.0,
        };
        let card = Card {
            id: 9,
            deck_id: 1,
            front: "front".into(),
            back: "back".into(),
            phase: Phase::Relearning,
            learning_step: Some(0),
            memory: Some(memory),
            due: Some(now),
            last_review: Some(now - Duration::minutes(10)),
        };
        let (updated, _) = card.apply_rating(Rating::Again, now).unwrap();
        assert_eq!(updated.phase, Phase::Relearning);
        assert_eq!(updated.learning_step, Some(0));
        assert_eq!(updated.due, Some(now + RELEARNING_STEPS[0]));
        assert_eq!(updated.memory, Some(memory));
    }
}
