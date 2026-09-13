//! Users, Sessions, and owner-scoped Deck/Card queries.

use chrono::{DateTime, Utc};
use domain::{Card, CardText, Deck, Session, User, UserId};

use crate::{
    CardRow, Error, SqlitePool, card_from_row, is_foreign_key_violation, is_unique_violation,
    map_missing_parent_deck, normalize_card_side, normalize_deck_name, parse_utc, phase_from_db,
    rfc3339,
};

/// Deck/Card ops run only for a real `users` row. Missing ids are not a
/// special scope: reads return empty, writes return `UserNotFound`.
pub(crate) async fn existing_user(
    pool: &SqlitePool,
    user_id: UserId,
) -> Result<Option<UserId>, Error> {
    Ok(get_user(pool, user_id).await?.map(|user| user.id))
}

pub(crate) async fn require_user(pool: &SqlitePool, user_id: UserId) -> Result<UserId, Error> {
    existing_user(pool, user_id)
        .await?
        .ok_or(Error::UserNotFound { user_id })
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
    user_id: UserId,
    deck_id: i64,
) -> Result<Option<Deck>, Error> {
    if existing_user(pool, user_id).await?.is_none() {
        return Ok(None);
    }
    let row = sqlx::query_as::<_, (i64, String)>(
        "SELECT id, name FROM decks WHERE id = ? AND user_id = ?",
    )
    .bind(deck_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(id, name)| Deck { id, name }))
}

pub(crate) async fn list_owned_decks(
    pool: &SqlitePool,
    user_id: UserId,
) -> Result<Vec<Deck>, Error> {
    if existing_user(pool, user_id).await?.is_none() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query_as::<_, (i64, String)>(
        "SELECT id, name FROM decks WHERE user_id = ? ORDER BY id",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, name)| Deck { id, name })
        .collect())
}

pub(crate) async fn create_owned_deck(
    pool: &SqlitePool,
    user_id: UserId,
    name: &str,
) -> Result<Deck, Error> {
    require_user(pool, user_id).await?;
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
                return Err(Error::UserNotFound { user_id });
            }
            Err(err) => return Err(Error::from(err)),
        };
    Ok(Deck { id, name })
}

pub(crate) async fn rename_owned_deck(
    pool: &SqlitePool,
    user_id: UserId,
    deck_id: i64,
    name: &str,
) -> Result<Deck, Error> {
    require_user(pool, user_id).await?;
    let name = normalize_deck_name(name)?;
    let result = sqlx::query("UPDATE decks SET name = ? WHERE id = ? AND user_id = ?")
        .bind(&name)
        .bind(deck_id)
        .bind(user_id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(Error::DeckNotFound { deck_id });
    }
    Ok(Deck { id: deck_id, name })
}

pub(crate) async fn delete_owned_deck(
    pool: &SqlitePool,
    user_id: UserId,
    deck_id: i64,
) -> Result<(), Error> {
    require_user(pool, user_id).await?;
    let result = sqlx::query("DELETE FROM decks WHERE id = ? AND user_id = ?")
        .bind(deck_id)
        .bind(user_id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(Error::DeckNotFound { deck_id });
    }
    Ok(())
}

pub(crate) async fn get_owned_card(
    pool: &SqlitePool,
    user_id: UserId,
    card_id: i64,
) -> Result<Option<Card>, Error> {
    if existing_user(pool, user_id).await?.is_none() {
        return Ok(None);
    }
    let row = sqlx::query_as::<_, CardRow>(
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
    .await?;
    row.map(card_from_row).transpose()
}

pub(crate) async fn list_owned_card_text(
    pool: &SqlitePool,
    user_id: UserId,
    deck_id: i64,
) -> Result<Vec<CardText>, Error> {
    if get_owned_deck(pool, user_id, deck_id).await?.is_none() {
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
    user_id: UserId,
    deck_id: i64,
    front: &str,
    back: &str,
) -> Result<Card, Error> {
    require_user(pool, user_id).await?;
    if get_owned_deck(pool, user_id, deck_id).await?.is_none() {
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
    user_id: UserId,
    card_id: i64,
    front: &str,
    back: &str,
) -> Result<Card, Error> {
    require_user(pool, user_id).await?;
    if get_owned_card(pool, user_id, card_id).await?.is_none() {
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
    user_id: UserId,
    card_id: i64,
) -> Result<i64, Error> {
    require_user(pool, user_id).await?;
    if get_owned_card(pool, user_id, card_id).await?.is_none() {
        return Err(Error::CardNotFound { card_id });
    }
    sqlx::query_scalar("DELETE FROM cards WHERE id = ? RETURNING deck_id")
        .bind(card_id)
        .fetch_optional(pool)
        .await?
        .ok_or(Error::CardNotFound { card_id })
}
