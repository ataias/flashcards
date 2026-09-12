use chrono::{DateTime, TimeZone, Utc};

use crate::DeckSummary;
use crate::queue::{capped_new_count_for_local_day, is_due};
use crate::store::HomeInputs;

/// Due counts plus the same local-day new-card cap used by Study.
pub fn summarize_home<Tz: TimeZone>(inputs: &HomeInputs, now: DateTime<Tz>) -> Vec<DeckSummary> {
    let now_utc = now.with_timezone(&Utc);
    let tz = now.timezone();
    inputs
        .decks
        .iter()
        .map(|deck_input| {
            let due_count = deck_input
                .cards
                .iter()
                .filter(|card| is_due(card, now_utc))
                .count();
            let uncapped_new = deck_input.cards.iter().filter(|card| card.is_new()).count();
            let firsts: Vec<_> = deck_input
                .first_reviewed_at
                .iter()
                .map(|instant| instant.with_timezone(&tz))
                .collect();
            DeckSummary {
                deck: deck_input.deck.clone(),
                due_count,
                new_count: capped_new_count_for_local_day(uncapped_new, &firsts, &now),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queue::NEW_CARDS_PER_LOCAL_DAY;
    use crate::store::HomeDeckInput;
    use crate::{Card, Deck};
    use chrono::{Duration, FixedOffset, TimeZone};
    use fsrs::MemoryState;

    fn tz_plus_9() -> FixedOffset {
        FixedOffset::east_opt(9 * 3600).unwrap()
    }

    fn deck(id: i64, name: &str) -> Deck {
        Deck {
            id,
            name: name.into(),
        }
    }

    fn new_card(id: i64, deck_id: i64) -> Card {
        Card {
            id,
            deck_id,
            front: "front".into(),
            back: "back".into(),
            phase: crate::Phase::New,
            learning_step: None,
            memory: None,
            due: None,
            last_review: None,
        }
    }

    fn reviewed_card(id: i64, deck_id: i64, due: chrono::DateTime<Utc>) -> Card {
        Card {
            id,
            deck_id,
            front: "front".into(),
            back: "back".into(),
            phase: crate::Phase::Review,
            learning_step: None,
            memory: Some(MemoryState {
                stability: 2.0,
                difficulty: 5.0,
            }),
            due: Some(due),
            last_review: Some(due - Duration::days(3)),
        }
    }

    fn learning_card(id: i64, deck_id: i64, due: chrono::DateTime<Utc>) -> Card {
        Card {
            id,
            deck_id,
            front: "front".into(),
            back: "back".into(),
            phase: crate::Phase::Learning,
            learning_step: Some(0),
            memory: None,
            due: Some(due),
            last_review: Some(due - crate::LEARNING_STEPS[0]),
        }
    }

    fn home_deck(
        deck: Deck,
        cards: Vec<Card>,
        first_reviewed_at: Vec<chrono::DateTime<Utc>>,
    ) -> HomeDeckInput {
        HomeDeckInput {
            deck,
            cards,
            first_reviewed_at,
        }
    }

    #[test]
    fn empty_inputs_yield_empty_summaries() {
        let now = tz_plus_9().with_ymd_and_hms(2026, 9, 11, 12, 0, 0).unwrap();
        let summaries = summarize_home(&HomeInputs { decks: vec![] }, now);
        assert!(summaries.is_empty());
    }

    #[test]
    fn deck_with_no_cards_has_zero_counts() {
        let now = tz_plus_9().with_ymd_and_hms(2026, 9, 11, 12, 0, 0).unwrap();
        let inputs = HomeInputs {
            decks: vec![home_deck(deck(1, "Default"), vec![], vec![])],
        };
        let summaries = summarize_home(&inputs, now);
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].due_count, 0);
        assert_eq!(summaries[0].new_count, 0);
    }

    #[test]
    fn new_count_is_capped_at_twenty_per_local_day() {
        let now = tz_plus_9().with_ymd_and_hms(2026, 9, 11, 12, 0, 0).unwrap();
        let cards: Vec<_> = (1..=25).map(|id| new_card(id, 1)).collect();
        let inputs = HomeInputs {
            decks: vec![home_deck(deck(1, "Default"), cards, vec![])],
        };
        let summaries = summarize_home(&inputs, now);
        assert_eq!(summaries[0].new_count, NEW_CARDS_PER_LOCAL_DAY);
        assert_eq!(summaries[0].due_count, 0);
    }

    #[test]
    fn introductions_on_local_day_reduce_remaining_new_slots() {
        let tz = tz_plus_9();
        let now = tz.with_ymd_and_hms(2026, 9, 11, 12, 0, 0).unwrap();
        let today = tz
            .with_ymd_and_hms(2026, 9, 11, 9, 0, 0)
            .unwrap()
            .with_timezone(&Utc);
        let cards: Vec<_> = (1..=10).map(|id| new_card(id, 1)).collect();
        let inputs = HomeInputs {
            decks: vec![home_deck(deck(1, "Default"), cards, vec![today; 17])],
        };
        let summaries = summarize_home(&inputs, now);
        assert_eq!(summaries[0].new_count, 3);
    }

    #[test]
    fn introductions_before_local_midnight_do_not_consume_today_cap() {
        let tz = tz_plus_9();
        let now = tz.with_ymd_and_hms(2026, 9, 11, 0, 30, 0).unwrap();
        let yesterday = tz
            .with_ymd_and_hms(2026, 9, 10, 23, 30, 0)
            .unwrap()
            .with_timezone(&Utc);
        let cards: Vec<_> = (1..=20).map(|id| new_card(id, 1)).collect();
        let inputs = HomeInputs {
            decks: vec![home_deck(
                deck(1, "Default"),
                cards,
                vec![yesterday; NEW_CARDS_PER_LOCAL_DAY],
            )],
        };
        let summaries = summarize_home(&inputs, now);
        assert_eq!(summaries[0].new_count, NEW_CARDS_PER_LOCAL_DAY);
    }

    #[test]
    fn due_cards_are_counted_when_new_cap_is_exhausted() {
        let tz = tz_plus_9();
        let now = tz.with_ymd_and_hms(2026, 9, 11, 12, 0, 0).unwrap();
        let now_utc = now.with_timezone(&Utc);
        let due = reviewed_card(99, 1, now_utc - Duration::hours(1));
        let new = new_card(1, 1);
        let introductions = vec![now_utc; NEW_CARDS_PER_LOCAL_DAY];
        let inputs = HomeInputs {
            decks: vec![home_deck(deck(1, "Default"), vec![due, new], introductions)],
        };
        let summaries = summarize_home(&inputs, now);
        assert_eq!(summaries[0].due_count, 1);
        assert_eq!(summaries[0].new_count, 0);
    }

    #[test]
    fn not_yet_due_cards_are_excluded_from_due_count() {
        let tz = tz_plus_9();
        let now = tz.with_ymd_and_hms(2026, 9, 11, 12, 0, 0).unwrap();
        let later = reviewed_card(7, 1, now.with_timezone(&Utc) + Duration::hours(1));
        let inputs = HomeInputs {
            decks: vec![home_deck(deck(1, "Default"), vec![later], vec![])],
        };
        let summaries = summarize_home(&inputs, now);
        assert_eq!(summaries[0].due_count, 0);
        assert_eq!(summaries[0].new_count, 0);
    }

    #[test]
    fn learning_card_due_now_counts_as_due_not_new() {
        let tz = tz_plus_9();
        let now = tz.with_ymd_and_hms(2026, 9, 11, 12, 0, 0).unwrap();
        let now_utc = now.with_timezone(&Utc);
        let due = learning_card(2, 1, now_utc);
        let later = learning_card(3, 1, now_utc + Duration::minutes(10));
        let new = new_card(1, 1);
        let inputs = HomeInputs {
            decks: vec![home_deck(
                deck(1, "Default"),
                vec![due, later, new],
                vec![now_utc],
            )],
        };
        let summaries = summarize_home(&inputs, now);
        assert_eq!(summaries[0].due_count, 1);
        assert_eq!(summaries[0].new_count, 1);
    }

    #[test]
    fn each_deck_applies_the_new_cap_independently() {
        let now = tz_plus_9().with_ymd_and_hms(2026, 9, 11, 12, 0, 0).unwrap();
        let default_cards: Vec<_> = (1..=25).map(|id| new_card(id, 1)).collect();
        let other_cards: Vec<_> = (100..=104).map(|id| new_card(id, 2)).collect();
        let inputs = HomeInputs {
            decks: vec![
                home_deck(deck(1, "Default"), default_cards, vec![]),
                home_deck(deck(2, "Spanish"), other_cards, vec![]),
            ],
        };
        let summaries = summarize_home(&inputs, now);
        assert_eq!(summaries[0].deck.name, "Default");
        assert_eq!(summaries[0].new_count, NEW_CARDS_PER_LOCAL_DAY);
        assert_eq!(summaries[1].deck.name, "Spanish");
        assert_eq!(summaries[1].new_count, 5);
    }
}
