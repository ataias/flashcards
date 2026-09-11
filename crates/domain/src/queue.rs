use chrono::{DateTime, TimeZone, Utc};

use crate::Card;

pub const NEW_CARDS_PER_LOCAL_DAY: usize = 20;

pub fn remaining_new_card_slots(introduced_today: usize) -> usize {
    NEW_CARDS_PER_LOCAL_DAY.saturating_sub(introduced_today)
}

pub fn apply_daily_new_cap(new_count: usize, introduced_today: usize) -> usize {
    new_count.min(remaining_new_card_slots(introduced_today))
}

/// Home-page New count: uncapped New Cards, then the local-day cap.
pub fn capped_new_count_for_local_day<Tz: TimeZone>(
    uncapped_new: usize,
    first_reviewed_at: &[DateTime<Tz>],
    now: &DateTime<Tz>,
) -> usize {
    apply_daily_new_cap(
        uncapped_new,
        new_cards_introduced_on_local_day(first_reviewed_at, now),
    )
}

pub fn is_due_at(due: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    due <= now
}

pub fn new_cards_introduced_on_local_day<Tz: TimeZone>(
    first_reviewed_at: &[DateTime<Tz>],
    now: &DateTime<Tz>,
) -> usize {
    let today = now.date_naive();
    first_reviewed_at
        .iter()
        .filter(|instant| instant.date_naive() == today)
        .count()
}

/// `now` must be in the machine-local timezone (local midnight boundary).
/// `first_reviewed_at` is each Card's first Review instant in that same zone.
pub fn select_study_queue<Tz: TimeZone>(
    cards: &[Card],
    first_reviewed_at: &[DateTime<Tz>],
    now: DateTime<Tz>,
) -> Vec<Card> {
    let now_utc = now.with_timezone(&Utc);
    let remaining_new =
        remaining_new_card_slots(new_cards_introduced_on_local_day(first_reviewed_at, &now));

    let mut due: Vec<Card> = cards
        .iter()
        .filter(|card| is_due(card, now_utc))
        .cloned()
        .collect();
    due.sort_by_key(|card| (card.due, card.id));

    let mut new_cards: Vec<Card> = cards.iter().filter(|card| card.is_new()).cloned().collect();
    new_cards.sort_by_key(|card| card.id);
    new_cards.truncate(remaining_new);

    due.extend(new_cards);
    due
}

fn is_due(card: &Card, now: DateTime<Utc>) -> bool {
    match (card.memory, card.due) {
        (Some(_), Some(due)) => is_due_at(due, now),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, FixedOffset, TimeZone, Utc};
    use fsrs::MemoryState;

    fn tz_plus_9() -> FixedOffset {
        FixedOffset::east_opt(9 * 3600).unwrap()
    }

    fn new_card(id: i64) -> Card {
        Card {
            id,
            deck_id: 1,
            front: "front".into(),
            back: "back".into(),
            memory: None,
            due: None,
            last_review: None,
        }
    }

    fn reviewed_card(id: i64, due: DateTime<Utc>) -> Card {
        Card {
            id,
            deck_id: 1,
            front: "front".into(),
            back: "back".into(),
            memory: Some(MemoryState {
                stability: 2.0,
                difficulty: 5.0,
            }),
            due: Some(due),
            last_review: Some(due - Duration::days(3)),
        }
    }

    fn ids(cards: &[Card]) -> Vec<i64> {
        cards.iter().map(|card| card.id).collect()
    }

    #[test]
    fn apply_daily_new_cap_floors_at_zero_and_twenty() {
        assert_eq!(apply_daily_new_cap(5, 0), 5);
        assert_eq!(apply_daily_new_cap(25, 0), NEW_CARDS_PER_LOCAL_DAY);
        assert_eq!(apply_daily_new_cap(5, NEW_CARDS_PER_LOCAL_DAY), 0);
        assert_eq!(apply_daily_new_cap(5, 17), 3);
    }

    #[test]
    fn capped_new_count_uses_local_day_introductions() {
        let tz = tz_plus_9();
        let now = tz.with_ymd_and_hms(2026, 9, 11, 12, 0, 0).unwrap();
        let today = tz.with_ymd_and_hms(2026, 9, 11, 9, 0, 0).unwrap();
        let yesterday = tz.with_ymd_and_hms(2026, 9, 10, 23, 30, 0).unwrap();
        assert_eq!(capped_new_count_for_local_day(10, &[today; 17], &now), 3);
        assert_eq!(
            capped_new_count_for_local_day(10, &[yesterday; NEW_CARDS_PER_LOCAL_DAY], &now),
            10
        );
    }

    #[test]
    fn empty_queue_when_no_cards() {
        let now = tz_plus_9().with_ymd_and_hms(2026, 9, 11, 12, 0, 0).unwrap();
        let queue = select_study_queue(&[], &[], now);
        assert!(queue.is_empty());
    }

    #[test]
    fn empty_queue_when_new_cap_exhausted_and_nothing_due() {
        let tz = tz_plus_9();
        let now = tz.with_ymd_and_hms(2026, 9, 11, 12, 0, 0).unwrap();
        let introductions: Vec<_> = (0..NEW_CARDS_PER_LOCAL_DAY)
            .map(|i| tz.with_ymd_and_hms(2026, 9, 11, 8, i as u32, 0).unwrap())
            .collect();
        let cards: Vec<_> = (1..=5).map(new_card).collect();
        let queue = select_study_queue(&cards, &introductions, now);
        assert!(queue.is_empty());
    }

    #[test]
    fn new_card_cap_is_twenty_per_local_day() {
        let tz = FixedOffset::east_opt(-5 * 3600).unwrap();
        let now = tz.with_ymd_and_hms(2026, 9, 11, 10, 0, 0).unwrap();
        let cards: Vec<_> = (1..=25).map(new_card).collect();
        let queue = select_study_queue(&cards, &[], now);
        assert_eq!(queue.len(), NEW_CARDS_PER_LOCAL_DAY);
        assert_eq!(ids(&queue), (1..=20).collect::<Vec<_>>());
    }

    #[test]
    fn introductions_before_local_midnight_do_not_consume_today_cap() {
        let tz = tz_plus_9();
        let now = tz.with_ymd_and_hms(2026, 9, 11, 0, 30, 0).unwrap();
        let yesterday = tz.with_ymd_and_hms(2026, 9, 10, 23, 30, 0).unwrap();
        let cards: Vec<_> = (1..=20).map(new_card).collect();
        let introductions = vec![yesterday; NEW_CARDS_PER_LOCAL_DAY];
        let queue = select_study_queue(&cards, &introductions, now);
        assert_eq!(queue.len(), NEW_CARDS_PER_LOCAL_DAY);
    }

    #[test]
    fn introductions_at_local_midnight_consume_today_cap() {
        let tz = tz_plus_9();
        let now = tz.with_ymd_and_hms(2026, 9, 11, 12, 0, 0).unwrap();
        let midnight = tz.with_ymd_and_hms(2026, 9, 11, 0, 0, 0).unwrap();
        let cards: Vec<_> = (1..=5).map(new_card).collect();
        let introductions = vec![midnight; NEW_CARDS_PER_LOCAL_DAY];
        let queue = select_study_queue(&cards, &introductions, now);
        assert!(queue.is_empty());
    }

    #[test]
    fn remaining_new_slots_after_partial_local_day() {
        let tz = tz_plus_9();
        let now = tz.with_ymd_and_hms(2026, 9, 11, 18, 0, 0).unwrap();
        let today = tz.with_ymd_and_hms(2026, 9, 11, 9, 0, 0).unwrap();
        let cards: Vec<_> = (1..=10).map(new_card).collect();
        let introductions = vec![today; 17];
        let queue = select_study_queue(&cards, &introductions, now);
        assert_eq!(ids(&queue), vec![1, 2, 3]);
    }

    #[test]
    fn due_cards_included_when_new_cap_exhausted() {
        let tz = tz_plus_9();
        let now = tz.with_ymd_and_hms(2026, 9, 11, 12, 0, 0).unwrap();
        let now_utc = now.with_timezone(&Utc);
        let due = reviewed_card(99, now_utc - Duration::hours(1));
        let new = new_card(1);
        let introductions = vec![now; NEW_CARDS_PER_LOCAL_DAY];
        let queue = select_study_queue(&[due, new], &introductions, now);
        assert_eq!(ids(&queue), vec![99]);
    }

    #[test]
    fn not_yet_due_cards_are_excluded() {
        let tz = tz_plus_9();
        let now = tz.with_ymd_and_hms(2026, 9, 11, 12, 0, 0).unwrap();
        let later = reviewed_card(7, now.with_timezone(&Utc) + Duration::hours(1));
        let queue = select_study_queue(&[later], &[], now);
        assert!(queue.is_empty());
    }

    #[test]
    fn due_cards_come_before_new_cards() {
        let tz = tz_plus_9();
        let now = tz.with_ymd_and_hms(2026, 9, 11, 12, 0, 0).unwrap();
        let due = reviewed_card(50, now.with_timezone(&Utc) - Duration::minutes(1));
        let new = new_card(1);
        let queue = select_study_queue(&[new, due], &[], now);
        assert_eq!(ids(&queue), vec![50, 1]);
    }
}
