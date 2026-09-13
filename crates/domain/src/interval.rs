use chrono::{DateTime, Duration, Utc};

use crate::Phase;

/// Anki-style relative next-interval label for a due delta.
///
/// Rules: `<1m` under one minute; else `Nm` / `Nh` / `Nd`; `Nmo` from 30 days;
/// `Ny` from 365 days. Units are whole (floored) counts.
pub fn humanize_due_delta(delta: Duration) -> String {
    if delta < Duration::minutes(1) {
        return "<1m".into();
    }
    let minutes = delta.num_minutes();
    if minutes < 60 {
        return format!("{minutes}m");
    }
    let hours = delta.num_hours();
    if hours < 24 {
        return format!("{hours}h");
    }
    let days = delta.num_days();
    if days < 30 {
        return format!("{days}d");
    }
    if days < 365 {
        return format!("{}mo", days / 30);
    }
    format!("{}y", days / 365)
}

/// Deck list due line: omit for New; `due now` if due ≤ now; else `due in …`.
pub fn card_list_due_label(
    phase: Phase,
    due: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> Option<String> {
    if phase == Phase::New {
        return None;
    }
    match due {
        Some(due) if due <= now => Some("due now".into()),
        Some(due) => Some(format!("due in {}", humanize_due_delta(due - now))),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn noon() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 11, 12, 0, 0).unwrap()
    }

    #[test]
    fn under_one_minute_is_less_than_one_m() {
        assert_eq!(humanize_due_delta(Duration::zero()), "<1m");
        assert_eq!(humanize_due_delta(Duration::seconds(59)), "<1m");
        assert_eq!(humanize_due_delta(Duration::seconds(-5)), "<1m");
    }

    #[test]
    fn minutes_hours_and_days() {
        assert_eq!(humanize_due_delta(Duration::minutes(1)), "1m");
        assert_eq!(humanize_due_delta(Duration::minutes(10)), "10m");
        assert_eq!(humanize_due_delta(Duration::minutes(59)), "59m");
        assert_eq!(humanize_due_delta(Duration::hours(1)), "1h");
        assert_eq!(humanize_due_delta(Duration::hours(23)), "23h");
        assert_eq!(humanize_due_delta(Duration::days(1)), "1d");
        assert_eq!(humanize_due_delta(Duration::days(29)), "29d");
    }

    #[test]
    fn months_from_thirty_days_years_from_365() {
        assert_eq!(humanize_due_delta(Duration::days(30)), "1mo");
        assert_eq!(humanize_due_delta(Duration::days(59)), "1mo");
        assert_eq!(humanize_due_delta(Duration::days(60)), "2mo");
        assert_eq!(humanize_due_delta(Duration::days(364)), "12mo");
        assert_eq!(humanize_due_delta(Duration::days(365)), "1y");
        assert_eq!(humanize_due_delta(Duration::days(729)), "1y");
        assert_eq!(humanize_due_delta(Duration::days(730)), "2y");
    }

    #[test]
    fn new_omits_due_even_when_a_timestamp_is_present() {
        let now = noon();
        assert_eq!(
            card_list_due_label(Phase::New, Some(now + Duration::days(3)), now),
            None
        );
        assert_eq!(card_list_due_label(Phase::New, None, now), None);
    }

    #[test]
    fn review_due_now_or_relative() {
        let now = noon();
        assert_eq!(
            card_list_due_label(Phase::Review, Some(now), now),
            Some("due now".into())
        );
        assert_eq!(
            card_list_due_label(Phase::Review, Some(now - Duration::hours(2)), now),
            Some("due now".into())
        );
        assert_eq!(
            card_list_due_label(Phase::Review, Some(now + Duration::days(3)), now),
            Some("due in 3d".into())
        );
        assert_eq!(
            card_list_due_label(Phase::Learning, Some(now + Duration::minutes(10)), now),
            Some("due in 10m".into())
        );
        assert_eq!(
            card_list_due_label(Phase::Relearning, Some(now + Duration::seconds(30)), now),
            Some("due in <1m".into())
        );
        assert_eq!(card_list_due_label(Phase::Review, None, now), None);
    }
}
