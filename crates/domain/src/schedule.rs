use chrono::{DateTime, Duration, Utc};
use fsrs::{FSRS, MemoryState};

use crate::Rating;

/// FSRS default desired retention (v1: default parameters, no optimizer).
pub const DESIRED_RETENTION: f32 = 0.9;

#[derive(Debug, Clone, PartialEq)]
pub struct ScheduledReview {
    pub memory: MemoryState,
    pub due: DateTime<Utc>,
    pub last_review: DateTime<Utc>,
}

#[derive(Debug)]
pub enum ScheduleError {
    Fsrs(fsrs::FSRSError),
}

impl std::fmt::Display for ScheduleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fsrs(err) => write!(f, "FSRS scheduling failed: {err}"),
        }
    }
}

impl std::error::Error for ScheduleError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Fsrs(err) => Some(err),
        }
    }
}

/// Next memory state and due instant for a Review-phase Rating (or graduation).
///
/// `memory` / `last_review` are `None` when graduating from Learning with no
/// prior FSRS state. The crate's float-day interval is used as a duration.
pub fn schedule(
    memory: Option<MemoryState>,
    last_review: Option<DateTime<Utc>>,
    rating: Rating,
    now: DateTime<Utc>,
) -> Result<ScheduledReview, ScheduleError> {
    let elapsed_days = match last_review {
        Some(prev) => (now - prev).num_days().max(0) as u32,
        None => 0,
    };
    let next_states = FSRS::default()
        .next_states(memory, DESIRED_RETENTION, elapsed_days)
        .map_err(ScheduleError::Fsrs)?;
    let item = match rating {
        Rating::Again => next_states.again,
        Rating::Hard => next_states.hard,
        Rating::Good => next_states.good,
        Rating::Easy => next_states.easy,
    };
    Ok(ScheduledReview {
        memory: item.memory,
        due: now + duration_from_fsrs_days(item.interval),
        last_review: now,
    })
}

fn duration_from_fsrs_days(interval_days: f32) -> Duration {
    let millis = (f64::from(interval_days) * 86_400_000.0).round() as i64;
    Duration::milliseconds(millis.max(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn noon() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 11, 12, 0, 0).unwrap()
    }

    fn expected_item(
        memory: Option<MemoryState>,
        elapsed_days: u32,
        rating: Rating,
    ) -> fsrs::ItemState {
        let next = FSRS::default()
            .next_states(memory, DESIRED_RETENTION, elapsed_days)
            .unwrap();
        match rating {
            Rating::Again => next.again,
            Rating::Hard => next.hard,
            Rating::Good => next.good,
            Rating::Easy => next.easy,
        }
    }

    #[test]
    fn good_on_new_card_sets_due_from_fsrs_interval() {
        let now = noon();
        let scheduled = schedule(None, None, Rating::Good, now).unwrap();
        let item = expected_item(None, 0, Rating::Good);
        assert_eq!(scheduled.memory, item.memory);
        assert_eq!(scheduled.last_review, now);
        assert_eq!(scheduled.due, now + duration_from_fsrs_days(item.interval));
        assert!(scheduled.due > now);
    }

    #[test]
    fn again_due_is_sooner_than_easy_on_new_card() {
        let now = noon();
        let again = schedule(None, None, Rating::Again, now).unwrap();
        let easy = schedule(None, None, Rating::Easy, now).unwrap();
        assert!(again.due <= easy.due);
        assert_ne!(again.memory, easy.memory);
    }

    #[test]
    fn review_interval_can_be_shorter_than_one_day() {
        let now = noon();
        let memory = MemoryState {
            stability: 0.1,
            difficulty: 10.0,
        };
        let scheduled = schedule(
            Some(memory),
            Some(now - Duration::days(1)),
            Rating::Again,
            now,
        )
        .unwrap();
        let item = expected_item(Some(memory), 1, Rating::Again);
        assert!(item.interval < 1.0);
        assert_eq!(scheduled.due, now + duration_from_fsrs_days(item.interval));
        assert!(scheduled.due < now + Duration::days(1));
        assert!(scheduled.due > now);
    }

    #[test]
    fn elapsed_days_since_last_review_used_for_next_due() {
        let last = Utc.with_ymd_and_hms(2026, 9, 1, 12, 0, 0).unwrap();
        let now = noon();
        let memory = MemoryState {
            stability: 5.0,
            difficulty: 5.0,
        };
        let scheduled = schedule(Some(memory), Some(last), Rating::Good, now).unwrap();
        let item = expected_item(Some(memory), 10, Rating::Good);
        assert_eq!(scheduled.memory, item.memory);
        assert_eq!(scheduled.due, now + duration_from_fsrs_days(item.interval));
    }

    #[test]
    fn rating_grades_map_to_fsrs_buttons() {
        assert_eq!(Rating::Again.as_grade(), 1);
        assert_eq!(Rating::Hard.as_grade(), 2);
        assert_eq!(Rating::Good.as_grade(), 3);
        assert_eq!(Rating::Easy.as_grade(), 4);
    }
}
