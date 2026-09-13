//! Users, Sessions, and owner-scoped Deck/Card queries.

use chrono::{DateTime, Utc};
use domain::{Card, CardText, Deck, Session, User, UserId};

use crate::{
    CardRow, Error, SqlitePool, card_from_row, is_foreign_key_violation, is_unique_violation,
    map_missing_parent_deck, normalize_card_side, normalize_deck_name, parse_utc, phase_from_db,
    rfc3339,
};

/// How Deck/Card rows are visible for a Store `user_id`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Scope {
    /// No users yet: operate on `user_id` NULL (first-open Default, pre-login UI).
    Orphans,
    User(UserId),
    /// `user_id` is not a row, but other Users exist — see nothing / reject writes.
    Unknown(UserId),
}

impl Scope {
    fn owner(self) -> Option<Option<UserId>> {
        match self {
            Self::User(id) => Some(Some(id)),
            Self::Orphans => Some(None),
            Self::Unknown(_) => None,
        }
    }
}

pub(crate) async fn scope(pool: &SqlitePool, user_id: UserId) -> Result<Scope, Error> {
    let (total, mine): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM users), (SELECT COUNT(*) FROM users WHERE id = ?)",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    if mine > 0 {
        Ok(Scope::User(user_id))
    } else if total == 0 {
        Ok(Scope::Orphans)
    } else {
        Ok(Scope::Unknown(user_id))
    }
}

type UserRow = (i64, String, String, i64, i64);

fn user_from_row((id, username, password_hash, admin, disabled): UserRow) -> User {
    User {
        id,
        username,
        password_hash,
        admin: admin != 0,
        disabled: disabled != 0,
    }
}

pub(crate) async fn get_user(pool: &SqlitePool, user_id: UserId) -> Result<Option<User>, Error> {
    let row = sqlx::query_as::<_, UserRow>(
        "SELECT id, username, password_hash, admin, disabled FROM users WHERE id = ?",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(user_from_row))
}

pub(crate) async fn get_user_by_username(
    pool: &SqlitePool,
    username: &str,
) -> Result<Option<User>, Error> {
    let row = sqlx::query_as::<_, UserRow>(
        "SELECT id, username, password_hash, admin, disabled FROM users WHERE username = ? COLLATE NOCASE",
    )
    .bind(username)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(user_from_row))
}

pub(crate) async fn list_users(pool: &SqlitePool) -> Result<Vec<User>, Error> {
    let rows = sqlx::query_as::<_, UserRow>(
        "SELECT id, username, password_hash, admin, disabled FROM users ORDER BY id",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(user_from_row).collect())
}

pub(crate) async fn create_user(
    pool: &SqlitePool,
    username: &str,
    password_hash: &str,
    admin: bool,
) -> Result<User, Error> {
    let row = match sqlx::query_as::<_, UserRow>(
        "INSERT INTO users (username, password_hash, admin, disabled)
         VALUES (?, ?, ?, 0)
         RETURNING id, username, password_hash, admin, disabled",
    )
    .bind(username)
    .bind(password_hash)
    .bind(if admin { 1 } else { 0 })
    .fetch_one(pool)
    .await
    {
        Ok(row) => row,
        Err(err) if is_unique_violation(&err) => return Err(Error::UsernameTaken),
        Err(err) => return Err(Error::from(err)),
    };
    Ok(user_from_row(row))
}

pub(crate) async fn set_password_hash(
    pool: &SqlitePool,
    user_id: UserId,
    password_hash: &str,
) -> Result<User, Error> {
    let row = sqlx::query_as::<_, UserRow>(
        "UPDATE users SET password_hash = ? WHERE id = ?
         RETURNING id, username, password_hash, admin, disabled",
    )
    .bind(password_hash)
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .ok_or(Error::UserNotFound { user_id })?;
    Ok(user_from_row(row))
}

pub(crate) async fn set_disabled(
    pool: &SqlitePool,
    user_id: UserId,
    disabled: bool,
) -> Result<User, Error> {
    let row = sqlx::query_as::<_, UserRow>(
        "UPDATE users SET disabled = ? WHERE id = ?
         RETURNING id, username, password_hash, admin, disabled",
    )
    .bind(if disabled { 1 } else { 0 })
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .ok_or(Error::UserNotFound { user_id })?;
    Ok(user_from_row(row))
}

pub(crate) async fn delete_user(pool: &SqlitePool, user_id: UserId) -> Result<(), Error> {
    let result = sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(user_id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(Error::UserNotFound { user_id });
    }
    Ok(())
}

pub(crate) async fn create_session(
    pool: &SqlitePool,
    user_id: UserId,
    now: DateTime<Utc>,
) -> Result<Session, Error> {
    let at = rfc3339(now);
    let row = match sqlx::query_as::<_, (String, i64, String, String)>(
        "INSERT INTO sessions (id, user_id, created_at, last_used_at)
         VALUES (lower(hex(randomblob(16))), ?, ?, ?)
         RETURNING id, user_id, created_at, last_used_at",
    )
    .bind(user_id)
    .bind(&at)
    .bind(&at)
    .fetch_one(pool)
    .await
    {
        Ok(row) => row,
        Err(err) if is_foreign_key_violation(&err) => {
            return Err(Error::UserNotFound { user_id });
        }
        Err(err) => return Err(Error::from(err)),
    };
    session_from_row(row)
}

pub(crate) async fn get_session(
    pool: &SqlitePool,
    session_id: &str,
) -> Result<Option<Session>, Error> {
    let row = sqlx::query_as::<_, (String, i64, String, String)>(
        "SELECT id, user_id, created_at, last_used_at FROM sessions WHERE id = ?",
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await?;
    row.map(session_from_row).transpose()
}

pub(crate) async fn delete_session(pool: &SqlitePool, session_id: &str) -> Result<(), Error> {
    sqlx::query("DELETE FROM sessions WHERE id = ?")
        .bind(session_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub(crate) async fn delete_sessions_for_user(
    pool: &SqlitePool,
    user_id: UserId,
) -> Result<(), Error> {
    sqlx::query("DELETE FROM sessions WHERE user_id = ?")
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub(crate) async fn assign_orphan_decks(
    pool: &SqlitePool,
    user_id: UserId,
) -> Result<usize, Error> {
    if get_user(pool, user_id).await?.is_none() {
        return Err(Error::UserNotFound { user_id });
    }
    let result = sqlx::query("UPDATE decks SET user_id = ? WHERE user_id IS NULL")
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() as usize)
}

fn session_from_row(
    (id, user_id, created_at, last_used_at): (String, i64, String, String),
) -> Result<Session, Error> {
    Ok(Session {
        id,
        user_id,
        created_at: parse_utc(&created_at)?,
        last_used_at: parse_utc(&last_used_at)?,
    })
}

pub(crate) async fn get_owned_deck(
    pool: &SqlitePool,
    scope: Scope,
    deck_id: i64,
) -> Result<Option<Deck>, Error> {
    let Some(owner) = scope.owner() else {
        return Ok(None);
    };
    let row = match owner {
        Some(user_id) => {
            sqlx::query_as::<_, (i64, String)>(
                "SELECT id, name FROM decks WHERE id = ? AND user_id = ?",
            )
            .bind(deck_id)
            .bind(user_id)
            .fetch_optional(pool)
            .await?
        }
        None => {
            sqlx::query_as::<_, (i64, String)>(
                "SELECT id, name FROM decks WHERE id = ? AND user_id IS NULL",
            )
            .bind(deck_id)
            .fetch_optional(pool)
            .await?
        }
    };
    Ok(row.map(|(id, name)| Deck { id, name }))
}

pub(crate) async fn list_owned_decks(pool: &SqlitePool, scope: Scope) -> Result<Vec<Deck>, Error> {
    let Some(owner) = scope.owner() else {
        return Ok(Vec::new());
    };
    let rows = match owner {
        Some(user_id) => {
            sqlx::query_as::<_, (i64, String)>(
                "SELECT id, name FROM decks WHERE user_id = ? ORDER BY id",
            )
            .bind(user_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query_as::<_, (i64, String)>(
                "SELECT id, name FROM decks WHERE user_id IS NULL ORDER BY id",
            )
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|(id, name)| Deck { id, name })
        .collect())
}

pub(crate) async fn create_owned_deck(
    pool: &SqlitePool,
    scope: Scope,
    name: &str,
) -> Result<Deck, Error> {
    let user_id = match scope {
        Scope::User(id) => Some(id),
        Scope::Orphans => None,
        Scope::Unknown(user_id) => return Err(Error::UserNotFound { user_id }),
    };
    let name = normalize_deck_name(name)?;
    let id =
        match sqlx::query_scalar("INSERT INTO decks (name, user_id) VALUES (?, ?) RETURNING id")
            .bind(&name)
            .bind(user_id)
            .fetch_one(pool)
            .await
        {
            Ok(id) => id,
            Err(err) if is_foreign_key_violation(&err) => {
                return Err(Error::UserNotFound {
                    user_id: user_id.unwrap_or_default(),
                });
            }
            Err(err) => return Err(Error::from(err)),
        };
    Ok(Deck { id, name })
}

pub(crate) async fn rename_owned_deck(
    pool: &SqlitePool,
    scope: Scope,
    deck_id: i64,
    name: &str,
) -> Result<Deck, Error> {
    let name = normalize_deck_name(name)?;
    let Some(owner) = scope.owner() else {
        return Err(Error::DeckNotFound { deck_id });
    };
    let result = match owner {
        Some(user_id) => {
            sqlx::query("UPDATE decks SET name = ? WHERE id = ? AND user_id = ?")
                .bind(&name)
                .bind(deck_id)
                .bind(user_id)
                .execute(pool)
                .await?
        }
        None => {
            sqlx::query("UPDATE decks SET name = ? WHERE id = ? AND user_id IS NULL")
                .bind(&name)
                .bind(deck_id)
                .execute(pool)
                .await?
        }
    };
    if result.rows_affected() == 0 {
        return Err(Error::DeckNotFound { deck_id });
    }
    Ok(Deck { id: deck_id, name })
}

pub(crate) async fn delete_owned_deck(
    pool: &SqlitePool,
    scope: Scope,
    deck_id: i64,
) -> Result<(), Error> {
    let Some(owner) = scope.owner() else {
        return Err(Error::DeckNotFound { deck_id });
    };
    let result = match owner {
        Some(user_id) => {
            sqlx::query("DELETE FROM decks WHERE id = ? AND user_id = ?")
                .bind(deck_id)
                .bind(user_id)
                .execute(pool)
                .await?
        }
        None => {
            sqlx::query("DELETE FROM decks WHERE id = ? AND user_id IS NULL")
                .bind(deck_id)
                .execute(pool)
                .await?
        }
    };
    if result.rows_affected() == 0 {
        return Err(Error::DeckNotFound { deck_id });
    }
    Ok(())
}

pub(crate) async fn get_owned_card(
    pool: &SqlitePool,
    scope: Scope,
    card_id: i64,
) -> Result<Option<Card>, Error> {
    let Some(owner) = scope.owner() else {
        return Ok(None);
    };
    let row = match owner {
        Some(user_id) => {
            sqlx::query_as::<_, CardRow>(
                "SELECT cards.id, cards.deck_id, cards.front, cards.back, cards.phase,
                        cards.learning_step, cards.stability, cards.difficulty, cards.due,
                        cards.last_review
                 FROM cards
                 INNER JOIN decks ON decks.id = cards.deck_id
                 WHERE cards.id = ? AND decks.user_id = ?",
            )
            .bind(card_id)
            .bind(user_id)
            .fetch_optional(pool)
            .await?
        }
        None => {
            sqlx::query_as::<_, CardRow>(
                "SELECT cards.id, cards.deck_id, cards.front, cards.back, cards.phase,
                        cards.learning_step, cards.stability, cards.difficulty, cards.due,
                        cards.last_review
                 FROM cards
                 INNER JOIN decks ON decks.id = cards.deck_id
                 WHERE cards.id = ? AND decks.user_id IS NULL",
            )
            .bind(card_id)
            .fetch_optional(pool)
            .await?
        }
    };
    row.map(card_from_row).transpose()
}

pub(crate) async fn list_owned_card_text(
    pool: &SqlitePool,
    scope: Scope,
    deck_id: i64,
) -> Result<Vec<CardText>, Error> {
    if get_owned_deck(pool, scope, deck_id).await?.is_none() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query_as::<_, (i64, String, String, String, Option<String>)>(
        "SELECT id, front, back, phase, due FROM cards WHERE deck_id = ? ORDER BY id",
    )
    .bind(deck_id)
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|(id, front, back, phase, due)| {
            Ok(CardText {
                id,
                front,
                back,
                phase: phase_from_db(id, &phase)?,
                due: due.as_deref().map(parse_utc).transpose()?,
            })
        })
        .collect()
}

pub(crate) async fn create_owned_card(
    pool: &SqlitePool,
    scope: Scope,
    deck_id: i64,
    front: &str,
    back: &str,
) -> Result<Card, Error> {
    if get_owned_deck(pool, scope, deck_id).await?.is_none() {
        return Err(Error::DeckNotFound { deck_id });
    }
    let front = normalize_card_side(front, true)?;
    let back = normalize_card_side(back, false)?;
    let id = match sqlx::query_scalar(
        "INSERT INTO cards (deck_id, front, back) VALUES (?, ?, ?) RETURNING id",
    )
    .bind(deck_id)
    .bind(&front)
    .bind(&back)
    .fetch_one(pool)
    .await
    {
        Ok(id) => id,
        Err(err) => return Err(map_missing_parent_deck(err, deck_id)),
    };
    Ok(Card {
        id,
        deck_id,
        front,
        back,
        phase: domain::Phase::New,
        learning_step: None,
        memory: None,
        due: None,
        last_review: None,
    })
}

pub(crate) async fn update_owned_card(
    pool: &SqlitePool,
    scope: Scope,
    card_id: i64,
    front: &str,
    back: &str,
) -> Result<Card, Error> {
    if get_owned_card(pool, scope, card_id).await?.is_none() {
        return Err(Error::CardNotFound { card_id });
    }
    let front = normalize_card_side(front, true)?;
    let back = normalize_card_side(back, false)?;
    let row = sqlx::query_as::<_, CardRow>(
        "UPDATE cards SET front = ?, back = ? WHERE id = ?
         RETURNING id, deck_id, front, back, phase, learning_step, stability, difficulty, due, last_review",
    )
    .bind(&front)
    .bind(&back)
    .bind(card_id)
    .fetch_optional(pool)
    .await?
    .ok_or(Error::CardNotFound { card_id })?;
    card_from_row(row)
}

pub(crate) async fn delete_owned_card(
    pool: &SqlitePool,
    scope: Scope,
    card_id: i64,
) -> Result<i64, Error> {
    if get_owned_card(pool, scope, card_id).await?.is_none() {
        return Err(Error::CardNotFound { card_id });
    }
    sqlx::query_scalar("DELETE FROM cards WHERE id = ? RETURNING deck_id")
        .bind(card_id)
        .fetch_optional(pool)
        .await?
        .ok_or(Error::CardNotFound { card_id })
}
