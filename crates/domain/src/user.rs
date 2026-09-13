use chrono::{DateTime, Duration, Utc};

use crate::{Error, UserId};

/// Seeded when each User is created, unless bootstrap assigned pre-v1.3 orphans.
pub const DEFAULT_DECK_NAME: &str = "Default";

pub const USERNAME_MIN_LEN: usize = 3;
pub const USERNAME_MAX_LEN: usize = 32;

/// Sliding idle timeout for Sessions (refresh on use).
pub const SESSION_IDLE: Duration = Duration::days(30);

pub type SessionId = String;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    pub id: UserId,
    pub username: String,
    pub password_hash: String,
    pub admin: bool,
    pub disabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub id: SessionId,
    pub user_id: UserId,
    pub created_at: DateTime<Utc>,
    pub last_used_at: DateTime<Utc>,
}

impl Session {
    pub fn is_idle_expired(&self, now: DateTime<Utc>) -> bool {
        now - self.last_used_at > SESSION_IDLE
    }
}

/// Trim, then enforce 3–32 chars of `[A-Za-z0-9_.-]`. Case is kept as entered.
pub fn normalize_username(username: &str) -> Result<String, Error> {
    let username = username.trim();
    if !(USERNAME_MIN_LEN..=USERNAME_MAX_LEN).contains(&username.len()) {
        return Err(Error::InvalidUsername);
    }
    if !username
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
    {
        return Err(Error::InvalidUsername);
    }
    Ok(username.to_string())
}

pub fn usernames_equal(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn username_accepts_length_and_charset() {
        assert_eq!(normalize_username("abc").unwrap(), "abc");
        assert_eq!(normalize_username("  Ab.C-1_  ").unwrap(), "Ab.C-1_");
        assert_eq!(normalize_username(&"a".repeat(32)).unwrap().len(), 32);
        assert_eq!(normalize_username("Admin").unwrap(), "Admin");
    }

    #[test]
    fn username_rejects_length_and_charset() {
        assert!(matches!(
            normalize_username("ab").unwrap_err(),
            Error::InvalidUsername
        ));
        assert!(matches!(
            normalize_username(&"a".repeat(33)).unwrap_err(),
            Error::InvalidUsername
        ));
        assert!(matches!(
            normalize_username("ab c").unwrap_err(),
            Error::InvalidUsername
        ));
        assert!(matches!(
            normalize_username("ab@c").unwrap_err(),
            Error::InvalidUsername
        ));
        assert!(matches!(
            normalize_username("ábcd").unwrap_err(),
            Error::InvalidUsername
        ));
        assert!(matches!(
            normalize_username("   ").unwrap_err(),
            Error::InvalidUsername
        ));
    }

    #[test]
    fn usernames_compare_case_insensitively() {
        assert!(usernames_equal("Admin", "admin"));
        assert!(!usernames_equal("admin", "adman"));
    }

    #[test]
    fn session_idle_expires_after_thirty_days() {
        let created = Utc.with_ymd_and_hms(2026, 9, 13, 12, 0, 0).unwrap();
        let session = Session {
            id: "s1".into(),
            user_id: 1,
            created_at: created,
            last_used_at: created,
        };
        assert!(!session.is_idle_expired(created + SESSION_IDLE));
        assert!(session.is_idle_expired(created + SESSION_IDLE + Duration::seconds(1)));
    }
}
