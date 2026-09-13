//! SQLx Store implementation, migrations, and queries.

use std::collections::HashMap;
use std::path::Path;

use chrono::{DateTime, SecondsFormat, TimeZone, Utc};
use domain::{ReviewLogEntry, ScheduleError};

pub use domain::{Card, CardText, Deck, DeckSummary, Phase, Rating, ScheduledReview};
use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

pub use sqlx::SqlitePool;

mod store;
mod users;
pub use store::SqliteStore;

pub const DEFAULT_DECK_NAME: &str = "Default";

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

#[derive(Debug)]
pub enum Error {
    CreateParent {
        path: std::path::PathBuf,
        source: std::io::Error,
    },
    Sqlx(sqlx::Error),
    Migrate(sqlx::migrate::MigrateError),
    Schedule(ScheduleError),
    InvalidTimestamp {
        value: String,
        source: chrono::ParseError,
    },
    IncompleteFsrs {
        card_id: i64,
    },
    InvalidPhase {
        card_id: i64,
        value: String,
    },
    InvalidLearningStep {
        card_id: i64,
        value: i64,
    },
    EmptyDeckName,
    EmptyCardFront,
    EmptyCardBack,
    DeckNotFound {
        deck_id: i64,
    },
    CardNotFound {
        card_id: i64,
    },
    UsernameTaken,
    UserNotFound {
        user_id: i64,
    },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CreateParent { path, source } => {
                write!(
                    f,
                    "failed to create directory for database ({}): {source}",
                    path.display()
                )
            }
            Self::Sqlx(err) => write!(f, "database error: {err}"),
            Self::Migrate(err) => write!(f, "database migration failed: {err}"),
            Self::Schedule(err) => write!(f, "{err}"),
            Self::InvalidTimestamp { value, source } => {
                write!(f, "invalid timestamp `{value}`: {source}")
            }
            Self::IncompleteFsrs { card_id } => {
                write!(f, "card {card_id} has incomplete FSRS fields")
            }
            Self::InvalidPhase { card_id, value } => {
                write!(f, "card {card_id} has invalid phase `{value}`")
            }
            Self::InvalidLearningStep { card_id, value } => {
                write!(f, "card {card_id} has invalid learning_step {value}")
            }
            Self::EmptyDeckName => write!(f, "Deck name cannot be empty"),
            Self::EmptyCardFront => write!(f, "Card front cannot be empty"),
            Self::EmptyCardBack => write!(f, "Card back cannot be empty"),
            Self::DeckNotFound { deck_id } => write!(f, "deck {deck_id} not found"),
            Self::CardNotFound { card_id } => write!(f, "card {card_id} not found"),
            Self::UsernameTaken => write!(f, "username is already taken"),
            Self::UserNotFound { user_id } => write!(f, "user {user_id} not found"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::CreateParent { source, .. } => Some(source),
            Self::Sqlx(err) => Some(err),
            Self::Migrate(err) => Some(err),
            Self::Schedule(err) => Some(err),
            Self::InvalidTimestamp { source, .. } => Some(source),
            Self::IncompleteFsrs { .. }
            | Self::InvalidPhase { .. }
            | Self::InvalidLearningStep { .. }
            | Self::EmptyDeckName
            | Self::EmptyCardFront
            | Self::EmptyCardBack
            | Self::DeckNotFound { .. }
            | Self::CardNotFound { .. }
            | Self::UsernameTaken
            | Self::UserNotFound { .. } => None,
        }
    }
}

impl From<sqlx::Error> for Error {
    fn from(err: sqlx::Error) -> Self {
        Self::Sqlx(err)
    }
}

impl From<sqlx::migrate::MigrateError> for Error {
    fn from(err: sqlx::migrate::MigrateError) -> Self {
        Self::Migrate(err)
    }
}

impl From<ScheduleError> for Error {
    fn from(err: ScheduleError) -> Self {
        Self::Schedule(err)
    }
}

impl From<Error> for domain::Error {
    fn from(err: Error) -> Self {
        match err {
            Error::EmptyDeckName => Self::EmptyDeckName,
            Error::EmptyCardFront => Self::EmptyCardFront,
            Error::EmptyCardBack => Self::EmptyCardBack,
            Error::DeckNotFound { deck_id } => Self::DeckNotFound { deck_id },
            Error::CardNotFound { card_id } => Self::CardNotFound { card_id },
            Error::UsernameTaken => Self::UsernameTaken,
            Error::UserNotFound { user_id } => Self::UserNotFound { user_id },
            Error::Schedule(err) => Self::Schedule(err),
            other => Self::storage(other),
        }
    }
}

pub async fn open(path: impl AsRef<Path>) -> Result<SqlitePool, Error> {
    let path = path.as_ref();
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|source| Error::CreateParent {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;
    let first_init = !has_applied_migrations(&pool).await?;
    MIGRATOR.run(&pool).await?;
    if first_init {
        ensure_default_deck(&pool).await?;
    }
    Ok(pool)
}

async fn has_applied_migrations(pool: &SqlitePool) -> Result<bool, Error> {
    let exists: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = '_sqlx_migrations'",
    )
    .fetch_one(pool)
    .await?;
    if exists == 0 {
        return Ok(false);
    }
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
        .fetch_one(pool)
        .await?;
    Ok(n > 0)
}

/// COUNT+INSERT run in one `BEGIN IMMEDIATE` transaction so concurrent `open`
/// cannot both observe an empty table and insert a second Default.
async fn ensure_default_deck(pool: &SqlitePool) -> Result<(), Error> {
    let mut conn = pool.acquire().await?;
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
    let result = seed_default_if_empty(&mut conn).await;
    finish_immediate(conn, result).await
}

/// Delete a Deck (Cards and Review logs cascade). Missing ids are `DeckNotFound`.
pub async fn delete_deck(pool: &SqlitePool, deck_id: i64) -> Result<(), Error> {
    let result = sqlx::query("DELETE FROM decks WHERE id = ?")
        .bind(deck_id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(Error::DeckNotFound { deck_id });
    }
    Ok(())
}

async fn seed_default_if_empty(
    conn: &mut sqlx::pool::PoolConnection<sqlx::Sqlite>,
) -> Result<(), Error> {
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM decks")
        .fetch_one(&mut **conn)
        .await?;
    if n == 0 {
        sqlx::query("INSERT INTO decks (name) VALUES (?)")
            .bind(DEFAULT_DECK_NAME)
            .execute(&mut **conn)
            .await?;
    }
    Ok(())
}

async fn finish_immediate<T>(
    mut conn: sqlx::pool::PoolConnection<sqlx::Sqlite>,
    result: Result<T, Error>,
) -> Result<T, Error> {
    match result {
        Ok(value) => {
            sqlx::query("COMMIT").execute(&mut *conn).await?;
            Ok(value)
        }
        Err(err) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            Err(err)
        }
    }
}

pub async fn list_decks(pool: &SqlitePool) -> Result<Vec<Deck>, Error> {
    let rows = sqlx::query_as::<_, (i64, String)>("SELECT id, name FROM decks ORDER BY id")
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(|(id, name)| Deck { id, name })
        .collect())
}

pub async fn get_deck(pool: &SqlitePool, deck_id: i64) -> Result<Option<Deck>, Error> {
    let row = sqlx::query_as::<_, (i64, String)>("SELECT id, name FROM decks WHERE id = ?")
        .bind(deck_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|(id, name)| Deck { id, name }))
}

/// Create a Deck. Leading/trailing whitespace is trimmed; empty names are rejected.
pub async fn create_deck(pool: &SqlitePool, name: &str) -> Result<Deck, Error> {
    let name = normalize_deck_name(name)?;
    let id = sqlx::query_scalar("INSERT INTO decks (name) VALUES (?) RETURNING id")
        .bind(&name)
        .fetch_one(pool)
        .await?;
    Ok(Deck { id, name })
}

/// Rename a Deck. Same name rules as [`create_deck`].
pub async fn rename_deck(pool: &SqlitePool, deck_id: i64, name: &str) -> Result<Deck, Error> {
    let name = normalize_deck_name(name)?;
    let result = sqlx::query("UPDATE decks SET name = ? WHERE id = ?")
        .bind(&name)
        .bind(deck_id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(Error::DeckNotFound { deck_id });
    }
    Ok(Deck { id: deck_id, name })
}

pub(crate) fn normalize_deck_name(name: &str) -> Result<String, Error> {
    let name = name.trim();
    if name.is_empty() {
        Err(Error::EmptyDeckName)
    } else {
        Ok(name.to_string())
    }
}

/// Home due/new counts in two queries (all Decks’ card rows + first Reviews),
/// not one `study_queue` per Deck.
pub async fn list_deck_summaries<Tz: TimeZone>(
    pool: &SqlitePool,
    now_local: DateTime<Tz>,
) -> Result<Vec<DeckSummary>, Error> {
    let now_utc = now_local.with_timezone(&Utc);
    let rows = sqlx::query_as::<_, (i64, String, Option<i64>, Option<String>, Option<String>)>(
        "SELECT decks.id, decks.name, cards.id, cards.phase, cards.due
         FROM decks
         LEFT JOIN cards ON cards.deck_id = decks.id
         ORDER BY decks.id",
    )
    .fetch_all(pool)
    .await?;

    let firsts = sqlx::query_as::<_, (i64, String)>(
        "SELECT cards.deck_id, MIN(review_logs.rated_at)
         FROM review_logs
         INNER JOIN cards ON cards.id = review_logs.card_id
         GROUP BY review_logs.card_id",
    )
    .fetch_all(pool)
    .await?;

    let tz = now_local.timezone();
    let mut firsts_by_deck: HashMap<i64, Vec<DateTime<Tz>>> = HashMap::new();
    for (deck_id, rated_at) in firsts {
        firsts_by_deck
            .entry(deck_id)
            .or_default()
            .push(parse_utc(&rated_at)?.with_timezone(&tz));
    }

    let mut order = Vec::new();
    let mut by_deck: HashMap<i64, DeckSummary> = HashMap::new();
    for (id, name, card_id, phase, due) in rows {
        let summary = by_deck.entry(id).or_insert_with(|| {
            order.push(id);
            DeckSummary {
                deck: Deck { id, name },
                due_count: 0,
                new_count: 0,
            }
        });
        match (card_id, phase.as_deref(), due.as_deref()) {
            (None, _, _) => {}
            (Some(_), Some("new"), _) => summary.new_count += 1,
            (Some(_), Some("learning" | "review" | "relearning"), Some(due)) => {
                if domain::is_due_at(parse_utc(due)?, now_utc) {
                    summary.due_count += 1;
                }
            }
            (Some(card_id), Some(phase), _) => {
                return Err(Error::InvalidPhase {
                    card_id,
                    value: phase.to_string(),
                });
            }
            (Some(card_id), _, _) => return Err(Error::IncompleteFsrs { card_id }),
        }
    }

    let mut summaries = Vec::with_capacity(order.len());
    for id in order {
        let Some(mut summary) = by_deck.remove(&id) else {
            continue;
        };
        let firsts = firsts_by_deck.get(&id).map(Vec::as_slice).unwrap_or(&[]);
        summary.new_count =
            domain::capped_new_count_for_local_day(summary.new_count, firsts, &now_local);
        summaries.push(summary);
    }
    Ok(summaries)
}

pub(crate) type CardRow = (
    i64,
    i64,
    String,
    String,
    String,
    Option<i64>,
    Option<f64>,
    Option<f64>,
    Option<String>,
    Option<String>,
);

pub async fn get_card(pool: &SqlitePool, card_id: i64) -> Result<Option<Card>, Error> {
    get_card_on(pool, card_id).await
}

async fn get_card_on<'e, E>(executor: E, card_id: i64) -> Result<Option<Card>, Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let row = sqlx::query_as::<_, CardRow>(
        "SELECT id, deck_id, front, back, phase, learning_step, stability, difficulty, due, last_review
         FROM cards WHERE id = ?",
    )
    .bind(card_id)
    .fetch_optional(executor)
    .await?;
    row.map(card_from_row).transpose()
}

pub async fn list_cards_in_deck(pool: &SqlitePool, deck_id: i64) -> Result<Vec<Card>, Error> {
    let rows = sqlx::query_as::<_, CardRow>(
        "SELECT id, deck_id, front, back, phase, learning_step, stability, difficulty, due, last_review
         FROM cards WHERE deck_id = ? ORDER BY id",
    )
    .bind(deck_id)
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(card_from_row).collect()
}

/// Deck page list: front/back plus phase and due, id order.
pub async fn list_card_text_in_deck(
    pool: &SqlitePool,
    deck_id: i64,
) -> Result<Vec<CardText>, Error> {
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

/// Create a New Card. Front/back are trimmed; empty sides are rejected.
///
/// Missing (or concurrently deleted) Decks are `DeckNotFound` via the FK,
/// not a pre-check that can race into a 500.
pub async fn create_card(
    pool: &SqlitePool,
    deck_id: i64,
    front: &str,
    back: &str,
) -> Result<Card, Error> {
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

/// Update a Card’s front/back. Same trim/empty rules as [`create_card`].
///
/// `RETURNING` is one statement so a concurrent delete cannot 404 after
/// a successful UPDATE.
pub async fn update_card(
    pool: &SqlitePool,
    card_id: i64,
    front: &str,
    back: &str,
) -> Result<Card, Error> {
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

pub async fn delete_card(pool: &SqlitePool, card_id: i64) -> Result<i64, Error> {
    sqlx::query_scalar("DELETE FROM cards WHERE id = ? RETURNING deck_id")
        .bind(card_id)
        .fetch_optional(pool)
        .await?
        .ok_or(Error::CardNotFound { card_id })
}

pub(crate) fn map_missing_parent_deck(err: sqlx::Error, deck_id: i64) -> Error {
    if is_foreign_key_violation(&err) {
        Error::DeckNotFound { deck_id }
    } else {
        Error::from(err)
    }
}

pub(crate) fn is_foreign_key_violation(err: &sqlx::Error) -> bool {
    let sqlx::Error::Database(db_err) = err else {
        return false;
    };
    db_err.code().as_deref() == Some("787")
        || db_err.message().contains("FOREIGN KEY constraint failed")
}

pub(crate) fn is_unique_violation(err: &sqlx::Error) -> bool {
    let sqlx::Error::Database(db_err) = err else {
        return false;
    };
    matches!(db_err.code().as_deref(), Some("2067" | "1555"))
        || db_err.message().contains("UNIQUE constraint failed")
}

pub(crate) fn normalize_card_side(text: &str, front: bool) -> Result<String, Error> {
    let text = text.trim();
    if text.is_empty() {
        Err(if front {
            Error::EmptyCardFront
        } else {
            Error::EmptyCardBack
        })
    } else {
        Ok(text.to_string())
    }
}

/// First Review instant per Card in the Deck (UTC), for the local-day new-card cap.
pub async fn first_review_times(
    pool: &SqlitePool,
    deck_id: i64,
) -> Result<Vec<DateTime<Utc>>, Error> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT MIN(review_logs.rated_at)
         FROM review_logs
         INNER JOIN cards ON cards.id = review_logs.card_id
         WHERE cards.deck_id = ?
         GROUP BY review_logs.card_id",
    )
    .bind(deck_id)
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(|(value,)| parse_utc(&value)).collect()
}

pub async fn study_queue<Tz: TimeZone>(
    pool: &SqlitePool,
    deck_id: i64,
    now_local: DateTime<Tz>,
) -> Result<Vec<Card>, Error> {
    let cards = list_cards_in_deck(pool, deck_id).await?;
    let first_reviews = first_review_times(pool, deck_id).await?;
    let tz = now_local.timezone();
    let first_local: Vec<_> = first_reviews
        .into_iter()
        .map(|instant| instant.with_timezone(&tz))
        .collect();
    Ok(domain::select_study_queue(&cards, &first_local, now_local))
}

/// Persist a Review: re-fetch the Card inside the write transaction, apply the
/// Rating to that row, then write the Card update and Review log together.
pub async fn commit_review(
    pool: &SqlitePool,
    card: &Card,
    entry: &ReviewLogEntry,
) -> Result<(Card, ReviewLogEntry), Error> {
    persist_review(pool, card.id, entry.rating, entry.rated_at).await
}

pub async fn rate_card(
    pool: &SqlitePool,
    card: &Card,
    rating: Rating,
    now: DateTime<Utc>,
) -> Result<Card, Error> {
    let (updated, _) = persist_review(pool, card.id, rating, now).await?;
    Ok(updated)
}

async fn persist_review(
    pool: &SqlitePool,
    card_id: i64,
    rating: Rating,
    now: DateTime<Utc>,
) -> Result<(Card, ReviewLogEntry), Error> {
    let mut tx = pool.begin().await?;
    let fresh = get_card_on(&mut *tx, card_id)
        .await?
        .ok_or(Error::CardNotFound { card_id })?;
    let (updated, entry) = fresh.apply_rating(rating, now)?;
    write_review_on(&mut tx, &updated, &entry).await?;
    tx.commit().await?;
    Ok((updated, entry))
}

async fn write_review_on(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    card: &Card,
    entry: &ReviewLogEntry,
) -> Result<(), Error> {
    let due = card.due.map(rfc3339);
    let last_review = card.last_review.map(rfc3339);
    let (stability, difficulty) = match card.memory {
        Some(memory) => (Some(memory.stability), Some(memory.difficulty)),
        None => (None, None),
    };
    sqlx::query(
        "UPDATE cards
         SET phase = ?, learning_step = ?, stability = ?, difficulty = ?, due = ?, last_review = ?
         WHERE id = ?",
    )
    .bind(phase_to_db(card.phase))
    .bind(card.learning_step.map(|step| step as i64))
    .bind(stability)
    .bind(difficulty)
    .bind(due.as_deref())
    .bind(last_review.as_deref())
    .bind(card.id)
    .execute(&mut **tx)
    .await?;
    sqlx::query("INSERT INTO review_logs (card_id, rated_at, rating) VALUES (?, ?, ?)")
        .bind(entry.card_id)
        .bind(rfc3339(entry.rated_at))
        .bind(entry.rating.as_grade())
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub(crate) fn rfc3339(instant: DateTime<Utc>) -> String {
    instant.to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub(crate) fn parse_utc(value: &str) -> Result<DateTime<Utc>, Error> {
    DateTime::parse_from_rfc3339(value)
        .map(|parsed| parsed.with_timezone(&Utc))
        .map_err(|source| Error::InvalidTimestamp {
            value: value.to_string(),
            source,
        })
}

fn phase_to_db(phase: domain::Phase) -> &'static str {
    match phase {
        domain::Phase::New => "new",
        domain::Phase::Learning => "learning",
        domain::Phase::Review => "review",
        domain::Phase::Relearning => "relearning",
    }
}

pub(crate) fn phase_from_db(card_id: i64, value: &str) -> Result<domain::Phase, Error> {
    match value {
        "new" => Ok(domain::Phase::New),
        "learning" => Ok(domain::Phase::Learning),
        "review" => Ok(domain::Phase::Review),
        "relearning" => Ok(domain::Phase::Relearning),
        _ => Err(Error::InvalidPhase {
            card_id,
            value: value.to_string(),
        }),
    }
}

fn learning_step_from_db(card_id: i64, value: Option<i64>) -> Result<Option<usize>, Error> {
    match value {
        None => Ok(None),
        Some(step) => usize::try_from(step)
            .map(Some)
            .map_err(|_| Error::InvalidLearningStep {
                card_id,
                value: step,
            }),
    }
}

pub(crate) fn card_from_row(row: CardRow) -> Result<Card, Error> {
    let (id, deck_id, front, back, phase, learning_step, stability, difficulty, due, last_review) =
        row;
    let phase = phase_from_db(id, &phase)?;
    let learning_step = learning_step_from_db(id, learning_step)?;
    let memory = match (stability, difficulty) {
        (None, None) => None,
        (Some(stability), Some(difficulty)) => Some(domain::MemoryState {
            stability: stability as f32,
            difficulty: difficulty as f32,
        }),
        _ => return Err(Error::IncompleteFsrs { card_id: id }),
    };
    let due = due.as_deref().map(parse_utc).transpose()?;
    let last_review = last_review.as_deref().map(parse_utc).transpose()?;
    Ok(Card {
        id,
        deck_id,
        front,
        back,
        phase,
        learning_step,
        memory,
        due,
        last_review,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use domain::{Card, Rating, Store, UserId, admin_create_user, bootstrap_admin};

    const USER: UserId = 1;
    use sqlx::sqlite::SqliteConnectOptions;

    fn noon() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 13, 12, 0, 0).unwrap()
    }

    async fn open_memory_migrations_only() -> SqlitePool {
        let options = SqliteConnectOptions::new()
            .in_memory(true)
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        MIGRATOR.run(&pool).await.unwrap();
        pool
    }

    async fn open_memory() -> SqlitePool {
        let pool = open_memory_migrations_only().await;
        ensure_default_deck(&pool).await.unwrap();
        pool
    }

    fn temp_db_path() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "flashcards-db-test-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn remove_db(path: &Path) {
        let _ = std::fs::remove_file(path);
        let wal = path.with_extension("db-wal");
        let shm = path.with_extension("db-shm");
        let _ = std::fs::remove_file(wal);
        let _ = std::fs::remove_file(shm);
        // filename is `*.db`; sidecar files are `*.db-wal` / `*.db-shm`.
        let _ = std::fs::remove_file(format!("{}-wal", path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", path.display()));
    }

    async fn insert_deck(pool: &SqlitePool, name: &str) -> i64 {
        sqlx::query_scalar("INSERT INTO decks (name) VALUES (?) RETURNING id")
            .bind(name)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    async fn insert_card(pool: &SqlitePool, deck_id: i64) -> i64 {
        sqlx::query_scalar(
            "INSERT INTO cards (deck_id, front, back) VALUES (?, 'front', 'back') RETURNING id",
        )
        .bind(deck_id)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    async fn insert_review_log(pool: &SqlitePool, card_id: i64) -> i64 {
        sqlx::query_scalar(
            "INSERT INTO review_logs (card_id, rated_at, rating) VALUES (?, '2026-09-11T00:00:00Z', 3) RETURNING id",
        )
        .bind(card_id)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    async fn count(pool: &SqlitePool, table: &str) -> i64 {
        let sql = format!("SELECT COUNT(*) FROM {table}");
        sqlx::query_scalar(&sql).fetch_one(pool).await.unwrap()
    }

    #[tokio::test]
    async fn fresh_file_db_is_created_and_seeded() {
        let path = temp_db_path();
        remove_db(&path);
        assert!(!path.exists());

        let pool = open(&path).await.unwrap();
        assert!(path.exists(), "SQLite file must be created on first open");
        let decks = list_decks(&pool).await.unwrap();
        assert_eq!(decks.len(), 1);
        assert_eq!(decks[0].name, DEFAULT_DECK_NAME);

        pool.close().await;
        remove_db(&path);
    }

    #[tokio::test]
    async fn reopen_does_not_duplicate_default() {
        let path = temp_db_path();
        remove_db(&path);

        let pool = open(&path).await.unwrap();
        pool.close().await;

        let pool = open(&path).await.unwrap();
        let decks = list_decks(&pool).await.unwrap();
        assert_eq!(decks.len(), 1);
        assert_eq!(decks[0].name, DEFAULT_DECK_NAME);

        pool.close().await;
        remove_db(&path);
    }

    #[tokio::test]
    async fn existing_empty_file_still_seeds_default() {
        let path = temp_db_path();
        remove_db(&path);
        std::fs::write(&path, []).unwrap();
        assert!(path.exists());

        let pool = open(&path).await.unwrap();
        let decks = list_decks(&pool).await.unwrap();
        assert_eq!(decks.len(), 1);
        assert_eq!(decks[0].name, DEFAULT_DECK_NAME);

        pool.close().await;
        remove_db(&path);
    }

    #[tokio::test]
    async fn reopen_after_last_delete_does_not_reseed() {
        let path = temp_db_path();
        remove_db(&path);

        let pool = open(&path).await.unwrap();
        let original_id = list_decks(&pool).await.unwrap()[0].id;
        delete_deck(&pool, original_id).await.unwrap();
        assert!(list_decks(&pool).await.unwrap().is_empty());
        pool.close().await;

        let pool = open(&path).await.unwrap();
        assert!(list_decks(&pool).await.unwrap().is_empty());

        pool.close().await;
        remove_db(&path);
    }

    #[tokio::test]
    async fn delete_last_deck_leaves_zero_decks() {
        let pool = open_memory().await;
        let original = list_decks(&pool).await.unwrap();
        assert_eq!(original.len(), 1);
        let original_id = original[0].id;

        delete_deck(&pool, original_id).await.unwrap();

        assert!(list_decks(&pool).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn delete_missing_deck_is_not_found() {
        let pool = open_memory().await;
        assert!(matches!(
            delete_deck(&pool, 999).await.unwrap_err(),
            Error::DeckNotFound { deck_id: 999 }
        ));
        assert_eq!(list_decks(&pool).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn delete_non_last_deck_does_not_recreate_default() {
        let pool = open_memory().await;
        let extra_id = insert_deck(&pool, "Spanish").await;
        let default_id = list_decks(&pool)
            .await
            .unwrap()
            .into_iter()
            .find(|d| d.name == DEFAULT_DECK_NAME)
            .unwrap()
            .id;

        delete_deck(&pool, default_id).await.unwrap();

        let decks = list_decks(&pool).await.unwrap();
        assert_eq!(decks.len(), 1);
        assert_eq!(decks[0].id, extra_id);
        assert_eq!(decks[0].name, "Spanish");
    }

    #[tokio::test]
    async fn delete_deck_cascades_cards_and_review_logs() {
        let pool = open_memory().await;
        let deck_id = list_decks(&pool).await.unwrap()[0].id;
        let card_id = insert_card(&pool, deck_id).await;
        insert_review_log(&pool, card_id).await;
        assert_eq!(count(&pool, "cards").await, 1);
        assert_eq!(count(&pool, "review_logs").await, 1);

        delete_deck(&pool, deck_id).await.unwrap();

        assert_eq!(count(&pool, "cards").await, 0);
        assert_eq!(count(&pool, "review_logs").await, 0);
        assert!(list_decks(&pool).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn delete_one_deck_does_not_cascade_other_deck_cards() {
        let pool = open_memory().await;
        let default_id = list_decks(&pool).await.unwrap()[0].id;
        let other_id = insert_deck(&pool, "Other").await;
        insert_card(&pool, default_id).await;
        let kept_card = insert_card(&pool, other_id).await;
        insert_review_log(&pool, kept_card).await;

        delete_deck(&pool, default_id).await.unwrap();

        assert_eq!(count(&pool, "cards").await, 1);
        assert_eq!(count(&pool, "review_logs").await, 1);
        let decks = list_decks(&pool).await.unwrap();
        assert_eq!(decks.len(), 1);
        assert_eq!(decks[0].id, other_id);
    }

    #[tokio::test]
    async fn rate_card_updates_memory_due_and_appends_review_log() {
        let pool = open_memory().await;
        let deck_id = list_decks(&pool).await.unwrap()[0].id;
        let card_id = insert_card(&pool, deck_id).await;
        let card = get_card(&pool, card_id).await.unwrap().unwrap();
        assert!(card.is_new());

        let now = Utc.with_ymd_and_hms(2026, 9, 11, 12, 0, 0).unwrap();
        let rated = rate_card(&pool, &card, Rating::Good, now).await.unwrap();

        let updated = get_card(&pool, card_id).await.unwrap().unwrap();
        assert!(!updated.is_new());
        assert_eq!(rated.phase, domain::Phase::Learning);
        assert_eq!(rated.learning_step, Some(1));
        assert_eq!(rated.memory, None);
        assert_eq!(updated.phase, domain::Phase::Learning);
        assert_eq!(updated.learning_step, Some(1));
        assert_eq!(updated.memory, None);
        assert_eq!(updated.due, Some(now + domain::LEARNING_STEPS[1]));
        assert_eq!(updated.last_review, Some(now));
        assert_eq!(updated.due, rated.due);
        let stability: Option<f64> = sqlx::query_scalar("SELECT stability FROM cards WHERE id = ?")
            .bind(card_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(stability, None);
        assert_eq!(count(&pool, "review_logs").await, 1);
    }

    #[tokio::test]
    async fn half_written_fsrs_row_is_rejected() {
        let pool = open_memory().await;
        let deck_id = list_decks(&pool).await.unwrap()[0].id;
        let err = sqlx::query(
            "INSERT INTO cards (deck_id, front, back, stability) VALUES (?, 'f', 'b', 1.2)",
        )
        .bind(deck_id)
        .execute(&pool)
        .await
        .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("CHECK") || msg.contains("constraint"),
            "expected CHECK failure, got {msg}"
        );
    }

    #[tokio::test]
    async fn due_without_last_review_is_rejected() {
        let pool = open_memory().await;
        let deck_id = list_decks(&pool).await.unwrap()[0].id;
        let err = sqlx::query(
            "INSERT INTO cards (deck_id, front, back, due) VALUES (?, 'f', 'b', '2026-09-11T00:00:00Z')",
        )
        .bind(deck_id)
        .execute(&pool)
        .await
        .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("CHECK") || msg.contains("constraint"),
            "expected CHECK failure, got {msg}"
        );
    }

    #[tokio::test]
    async fn study_queue_empty_when_deck_has_no_cards() {
        let pool = open_memory().await;
        let deck_id = list_decks(&pool).await.unwrap()[0].id;
        let now = chrono::FixedOffset::east_opt(0)
            .unwrap()
            .with_ymd_and_hms(2026, 9, 11, 12, 0, 0)
            .unwrap();
        let queue = study_queue(&pool, deck_id, now).await.unwrap();
        assert!(queue.is_empty());
    }

    #[tokio::test]
    async fn study_queue_caps_new_cards_on_local_day() {
        let pool = open_memory().await;
        let deck_id = list_decks(&pool).await.unwrap()[0].id;
        for _ in 0..25 {
            insert_card(&pool, deck_id).await;
        }
        let tz = chrono::FixedOffset::east_opt(9 * 3600).unwrap();
        let now = tz.with_ymd_and_hms(2026, 9, 11, 12, 0, 0).unwrap();
        let queue = study_queue(&pool, deck_id, now).await.unwrap();
        assert_eq!(queue.len(), domain::NEW_CARDS_PER_LOCAL_DAY);
        assert!(queue.iter().all(Card::is_new));
    }

    #[tokio::test]
    async fn create_deck_inserts_trimmed_name() {
        let pool = open_memory().await;
        let created = create_deck(&pool, "  Spanish  ").await.unwrap();
        assert_eq!(created.name, "Spanish");
        let fetched = get_deck(&pool, created.id).await.unwrap().unwrap();
        assert_eq!(fetched, created);
        let names: Vec<_> = list_decks(&pool)
            .await
            .unwrap()
            .into_iter()
            .map(|d| d.name)
            .collect();
        assert!(names.contains(&"Default".to_string()));
        assert!(names.contains(&"Spanish".to_string()));
    }

    #[tokio::test]
    async fn create_deck_rejects_empty_or_whitespace_name() {
        let pool = open_memory().await;
        assert!(matches!(
            create_deck(&pool, "   ").await.unwrap_err(),
            Error::EmptyDeckName
        ));
        assert_eq!(list_decks(&pool).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn rename_deck_updates_name() {
        let pool = open_memory().await;
        let id = list_decks(&pool).await.unwrap()[0].id;
        let renamed = rename_deck(&pool, id, "  Home  ").await.unwrap();
        assert_eq!(renamed.name, "Home");
        assert_eq!(list_decks(&pool).await.unwrap()[0].name, "Home");
    }

    #[tokio::test]
    async fn rename_missing_deck_is_not_found() {
        let pool = open_memory().await;
        assert!(matches!(
            rename_deck(&pool, 999, "Nope").await.unwrap_err(),
            Error::DeckNotFound { deck_id: 999 }
        ));
    }

    #[tokio::test]
    async fn list_deck_summaries_counts_due_and_new() {
        let pool = open_memory().await;
        let deck_id = list_decks(&pool).await.unwrap()[0].id;
        insert_card(&pool, deck_id).await;
        let card_id = insert_card(&pool, deck_id).await;
        let card = get_card(&pool, card_id).await.unwrap().unwrap();
        let now = Utc.with_ymd_and_hms(2026, 9, 11, 12, 0, 0).unwrap();
        rate_card(&pool, &card, Rating::Good, now - chrono::Duration::days(30))
            .await
            .unwrap();

        let summaries = list_deck_summaries(&pool, now).await.unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].new_count, 1);
        assert_eq!(summaries[0].due_count, 1);
    }

    #[tokio::test]
    async fn list_deck_summaries_caps_new_and_ignores_not_due() {
        let pool = open_memory().await;
        let default_id = list_decks(&pool).await.unwrap()[0].id;
        for _ in 0..25 {
            insert_card(&pool, default_id).await;
        }
        let other_id = insert_deck(&pool, "Later").await;
        let later_id = insert_card(&pool, other_id).await;
        let later = get_card(&pool, later_id).await.unwrap().unwrap();
        let now = Utc.with_ymd_and_hms(2026, 9, 11, 12, 0, 0).unwrap();
        rate_card(&pool, &later, Rating::Good, now).await.unwrap();

        let summaries = list_deck_summaries(&pool, now).await.unwrap();
        let default = summaries.iter().find(|s| s.deck.id == default_id).unwrap();
        let later_deck = summaries.iter().find(|s| s.deck.id == other_id).unwrap();
        assert_eq!(default.new_count, domain::NEW_CARDS_PER_LOCAL_DAY);
        assert_eq!(default.due_count, 0);
        assert_eq!(later_deck.new_count, 0);
        assert_eq!(later_deck.due_count, 0);
    }

    #[tokio::test]
    async fn create_update_delete_card() {
        let pool = open_memory().await;
        let deck_id = list_decks(&pool).await.unwrap()[0].id;
        let created = create_card(&pool, deck_id, "  Q  ", "  A  ").await.unwrap();
        assert_eq!(created.front, "Q");
        assert_eq!(created.back, "A");
        assert!(created.is_new());
        let listed = list_card_text_in_deck(&pool, deck_id).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, created.id);
        assert_eq!(listed[0].front, "Q");
        assert_eq!(listed[0].back, "A");
        assert_eq!(listed[0].phase, domain::Phase::New);
        assert_eq!(listed[0].due, None);

        let updated = update_card(&pool, created.id, "Q2", "A2").await.unwrap();
        assert_eq!(updated.front, "Q2");
        assert_eq!(updated.back, "A2");
        assert!(updated.is_new());

        let now = Utc.with_ymd_and_hms(2026, 9, 11, 12, 0, 0).unwrap();
        let rated = rate_card(&pool, &updated, Rating::Easy, now).await.unwrap();
        let edited = update_card(&pool, created.id, "Q3", "A3").await.unwrap();
        assert_eq!(edited.front, "Q3");
        assert_eq!(edited.back, "A3");
        assert_eq!(edited.phase, domain::Phase::Review);
        assert_eq!(edited.memory, rated.memory);
        assert_eq!(edited.due, rated.due);
        assert_eq!(edited.last_review, Some(now));

        let deleted_deck_id = delete_card(&pool, created.id).await.unwrap();
        assert_eq!(deleted_deck_id, deck_id);
        assert!(get_card(&pool, created.id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn create_card_rejects_empty_sides_and_missing_deck() {
        let pool = open_memory().await;
        let deck_id = list_decks(&pool).await.unwrap()[0].id;
        assert!(matches!(
            create_card(&pool, deck_id, "   ", "A").await.unwrap_err(),
            Error::EmptyCardFront
        ));
        assert!(matches!(
            create_card(&pool, deck_id, "Q", "  ").await.unwrap_err(),
            Error::EmptyCardBack
        ));
        assert!(matches!(
            create_card(&pool, 999, "Q", "A").await.unwrap_err(),
            Error::DeckNotFound { deck_id: 999 }
        ));
        assert_eq!(list_cards_in_deck(&pool, deck_id).await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn update_and_delete_missing_card_are_not_found() {
        let pool = open_memory().await;
        assert!(matches!(
            update_card(&pool, 999, "Q", "A").await.unwrap_err(),
            Error::CardNotFound { card_id: 999 }
        ));
        assert!(matches!(
            delete_card(&pool, 999).await.unwrap_err(),
            Error::CardNotFound { card_id: 999 }
        ));
    }

    #[tokio::test]
    async fn delete_card_cascades_review_logs() {
        let pool = open_memory().await;
        let deck_id = list_decks(&pool).await.unwrap()[0].id;
        let card_id = insert_card(&pool, deck_id).await;
        insert_review_log(&pool, card_id).await;
        delete_card(&pool, card_id).await.unwrap();
        assert_eq!(count(&pool, "cards").await, 0);
        assert_eq!(count(&pool, "review_logs").await, 0);
    }

    #[tokio::test]
    async fn rate_card_schedules_from_row_refetched_in_write_txn() {
        let pool = open_memory().await;
        let deck_id = list_decks(&pool).await.unwrap()[0].id;
        let card_id = insert_card(&pool, deck_id).await;
        let stale = get_card(&pool, card_id).await.unwrap().unwrap();
        assert!(stale.is_new());

        let now = Utc.with_ymd_and_hms(2026, 9, 11, 12, 0, 0).unwrap();
        rate_card(&pool, &stale, Rating::Easy, now).await.unwrap();
        let after_first = get_card(&pool, card_id).await.unwrap().unwrap();
        assert!(!after_first.is_new());
        assert_eq!(after_first.phase, domain::Phase::Review);

        let later = now + chrono::Duration::days(1);
        let expected = after_first.apply_rating(Rating::Good, later).unwrap().0;
        let from_stale = stale.apply_rating(Rating::Good, later).unwrap().0;
        assert_ne!(
            expected.due, from_stale.due,
            "stale New Card schedule must differ from persisted-state schedule"
        );

        let scheduled = rate_card(&pool, &stale, Rating::Good, later).await.unwrap();
        assert_eq!(scheduled.memory, expected.memory);
        assert_eq!(scheduled.due, expected.due);
        assert_eq!(scheduled.last_review, Some(later));
    }

    #[tokio::test]
    async fn store_commit_review_schedules_from_row_refetched_in_write_txn() {
        let store = SqliteStore::new(open_memory().await);
        let deck = domain::create_deck(&store, USER, "Default").await.unwrap();
        let stale = domain::create_card(&store, USER, deck.id, "Q", "A")
            .await
            .unwrap();
        assert!(stale.is_new());

        let now = Utc.with_ymd_and_hms(2026, 9, 11, 12, 0, 0).unwrap();
        let (first, first_entry) = stale.apply_rating(Rating::Easy, now).unwrap();
        store
            .commit_review(USER, &first, &first_entry)
            .await
            .unwrap();
        let after_first = domain::get_card(&store, USER, stale.id)
            .await
            .unwrap()
            .unwrap();

        let later = now + chrono::Duration::days(1);
        let expected = after_first.apply_rating(Rating::Good, later).unwrap().0;
        let (from_stale, stale_entry) = stale.apply_rating(Rating::Good, later).unwrap();
        assert_ne!(
            expected.due, from_stale.due,
            "stale New Card schedule must differ from persisted-state schedule"
        );

        let (persisted, persisted_entry) = store
            .commit_review(USER, &from_stale, &stale_entry)
            .await
            .unwrap();
        assert_eq!(persisted, expected);
        assert_ne!(persisted.due, from_stale.due);
        assert_eq!(persisted_entry.rating, Rating::Good);
        assert_eq!(persisted_entry.rated_at, later);
        let stored = domain::get_card(&store, USER, stale.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored, persisted);
    }

    #[tokio::test]
    async fn domain_rate_returns_card_that_was_persisted() {
        let store = SqliteStore::new(open_memory().await);
        let deck = domain::create_deck(&store, USER, "Default").await.unwrap();
        let stale = domain::create_card(&store, USER, deck.id, "Q", "A")
            .await
            .unwrap();

        let now = Utc.with_ymd_and_hms(2026, 9, 11, 12, 0, 0).unwrap();
        domain::rate(&store, USER, stale.id, Rating::Easy, now)
            .await
            .unwrap();
        let after_first = domain::get_card(&store, USER, stale.id)
            .await
            .unwrap()
            .unwrap();
        let later = now + chrono::Duration::days(1);
        let expected = after_first.apply_rating(Rating::Good, later).unwrap().0;
        let from_stale = stale.apply_rating(Rating::Good, later).unwrap().0;
        assert_ne!(expected.due, from_stale.due);

        let (rated, entry) = domain::rate(&store, USER, stale.id, Rating::Good, later)
            .await
            .unwrap();
        assert_eq!(rated, expected);
        assert_eq!(entry.rated_at, later);
        let stored = domain::get_card(&store, USER, stale.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored, rated);
    }

    #[tokio::test]
    async fn store_use_cases_round_trip_and_delete_last_deck() {
        let store = SqliteStore::new(open_memory().await);
        let seeded = domain::list_home(&store, USER, noon()).await.unwrap();
        assert_eq!(seeded.len(), 1);
        assert_eq!(seeded[0].deck.name, DEFAULT_DECK_NAME);

        let extra = domain::create_deck(&store, USER, "Spanish").await.unwrap();
        let card = domain::create_card(&store, USER, extra.id, "Q", "A")
            .await
            .unwrap();
        let now = Utc.with_ymd_and_hms(2026, 9, 11, 12, 0, 0).unwrap();
        domain::rate(&store, USER, card.id, Rating::Good, now)
            .await
            .unwrap();
        let stored = domain::get_card(&store, USER, card.id)
            .await
            .unwrap()
            .unwrap();
        assert!(!stored.is_new());
        assert_eq!(stored.phase, domain::Phase::Learning);
        assert_eq!(stored.learning_step, Some(1));
        assert_eq!(stored.memory, None);

        domain::delete_deck(&store, USER, seeded[0].deck.id)
            .await
            .unwrap();
        domain::delete_deck(&store, USER, extra.id).await.unwrap();
        assert!(
            domain::list_home(&store, USER, now)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(matches!(
            domain::delete_deck(&store, USER, extra.id).await.unwrap_err(),
            domain::Error::DeckNotFound { deck_id } if deck_id == extra.id
        ));
        assert!(matches!(
            domain::rate(&store, USER, card.id, Rating::Good, now)
                .await
                .unwrap_err(),
            domain::Error::CardNotFound { card_id } if card_id == card.id
        ));
    }

    async fn apply_migrations_through(pool: &SqlitePool, version: i64) {
        use sqlx::migrate::Migrate;
        let mut conn = pool.acquire().await.unwrap();
        conn.ensure_migrations_table().await.unwrap();
        for migration in MIGRATOR.iter() {
            if migration.version > version {
                break;
            }
            conn.apply(migration).await.unwrap();
        }
    }

    #[tokio::test]
    async fn get_card_after_rate_keeps_learning_phase_and_step() {
        let pool = open_memory().await;
        let deck_id = list_decks(&pool).await.unwrap()[0].id;
        let card_id = insert_card(&pool, deck_id).await;
        let card = get_card(&pool, card_id).await.unwrap().unwrap();
        let now = Utc.with_ymd_and_hms(2026, 9, 11, 12, 0, 0).unwrap();

        let rated = rate_card(&pool, &card, Rating::Again, now).await.unwrap();
        let stored = get_card(&pool, card_id).await.unwrap().unwrap();
        assert_eq!(rated.phase, domain::Phase::Learning);
        assert_eq!(rated.learning_step, Some(0));
        assert_eq!(rated.memory, None);
        assert_eq!(stored, rated);
        assert_eq!(stored.due, Some(now + domain::LEARNING_STEPS[0]));

        let later = now + domain::LEARNING_STEPS[0];
        let advanced = rate_card(&pool, &stored, Rating::Good, later)
            .await
            .unwrap();
        let stored = get_card(&pool, card_id).await.unwrap().unwrap();
        assert_eq!(advanced.phase, domain::Phase::Learning);
        assert_eq!(advanced.learning_step, Some(1));
        assert_eq!(advanced.memory, None);
        assert_eq!(stored, advanced);
    }

    #[tokio::test]
    async fn get_card_after_review_again_keeps_relearning() {
        let pool = open_memory().await;
        let deck_id = list_decks(&pool).await.unwrap()[0].id;
        let card_id = insert_card(&pool, deck_id).await;
        let card = get_card(&pool, card_id).await.unwrap().unwrap();
        let now = Utc.with_ymd_and_hms(2026, 9, 11, 12, 0, 0).unwrap();
        rate_card(&pool, &card, Rating::Easy, now).await.unwrap();
        let review = get_card(&pool, card_id).await.unwrap().unwrap();
        assert_eq!(review.phase, domain::Phase::Review);

        let later = now + chrono::Duration::days(1);
        let rated = rate_card(&pool, &review, Rating::Again, later)
            .await
            .unwrap();
        let stored = get_card(&pool, card_id).await.unwrap().unwrap();
        assert_eq!(rated.phase, domain::Phase::Relearning);
        assert_eq!(rated.learning_step, Some(0));
        assert!(rated.memory.is_some());
        assert_eq!(rated.due, Some(later + domain::RELEARNING_STEPS[0]));
        assert_eq!(stored, rated);
    }

    #[tokio::test]
    async fn store_commit_review_round_trips_learning_fields() {
        let store = SqliteStore::new(open_memory().await);
        let deck = domain::create_deck(&store, USER, "Default").await.unwrap();
        let card = domain::create_card(&store, USER, deck.id, "Q", "A")
            .await
            .unwrap();
        let now = Utc.with_ymd_and_hms(2026, 9, 11, 12, 0, 0).unwrap();
        let (updated, entry) = card.apply_rating(Rating::Good, now).unwrap();
        store.commit_review(USER, &updated, &entry).await.unwrap();

        let stored = domain::get_card(&store, USER, card.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.phase, domain::Phase::Learning);
        assert_eq!(stored.learning_step, Some(1));
        assert_eq!(stored.memory, None);
        assert_eq!(stored.due, Some(now + domain::LEARNING_STEPS[1]));
        assert_eq!(stored.last_review, Some(now));
    }

    #[tokio::test]
    async fn open_backfills_phase_from_memory_and_does_not_reseed() {
        let path = temp_db_path();
        remove_db(&path);

        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        apply_migrations_through(&pool, 2).await;
        ensure_default_deck(&pool).await.unwrap();
        let deck_id = list_decks(&pool).await.unwrap()[0].id;
        let new_id = insert_card(&pool, deck_id).await;
        let review_id: i64 = sqlx::query_scalar(
            "INSERT INTO cards (deck_id, front, back, stability, difficulty, due, last_review)
             VALUES (?, 'q', 'a', 2.0, 5.0, '2026-09-11T12:00:00Z', '2026-09-10T12:00:00Z')
             RETURNING id",
        )
        .bind(deck_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        pool.close().await;

        let pool = open(&path).await.unwrap();
        let decks = list_decks(&pool).await.unwrap();
        assert_eq!(decks.len(), 1);
        assert_eq!(decks[0].name, DEFAULT_DECK_NAME);

        let new_card = get_card(&pool, new_id).await.unwrap().unwrap();
        assert_eq!(new_card.phase, domain::Phase::New);
        assert_eq!(new_card.learning_step, None);
        assert_eq!(new_card.memory, None);

        let review = get_card(&pool, review_id).await.unwrap().unwrap();
        assert_eq!(review.phase, domain::Phase::Review);
        assert_eq!(review.learning_step, None);
        assert!(review.memory.is_some());

        pool.close().await;
        remove_db(&path);
    }

    #[tokio::test]
    async fn learning_due_without_memory_is_allowed() {
        let pool = open_memory().await;
        let deck_id = list_decks(&pool).await.unwrap()[0].id;
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO cards (deck_id, front, back, phase, learning_step, due, last_review)
             VALUES (?, 'f', 'b', 'learning', 0, '2026-09-11T12:01:00Z', '2026-09-11T12:00:00Z')
             RETURNING id",
        )
        .bind(deck_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let card = get_card(&pool, id).await.unwrap().unwrap();
        assert_eq!(card.phase, domain::Phase::Learning);
        assert_eq!(card.learning_step, Some(0));
        assert_eq!(card.memory, None);
    }

    #[tokio::test]
    async fn review_without_memory_is_rejected() {
        let pool = open_memory().await;
        let deck_id = list_decks(&pool).await.unwrap()[0].id;
        let err = sqlx::query(
            "INSERT INTO cards (deck_id, front, back, phase, due, last_review)
             VALUES (?, 'f', 'b', 'review', '2026-09-11T12:00:00Z', '2026-09-11T12:00:00Z')",
        )
        .bind(deck_id)
        .execute(&pool)
        .await
        .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("CHECK") || msg.contains("constraint"),
            "expected CHECK failure, got {msg}"
        );
    }

    async fn deck_user_id(pool: &SqlitePool, deck_id: i64) -> Option<i64> {
        sqlx::query_scalar("SELECT user_id FROM decks WHERE id = ?")
            .bind(deck_id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn users_migration_leaves_existing_decks_orphaned() {
        let path = temp_db_path();
        remove_db(&path);

        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        apply_migrations_through(&pool, 3).await;
        ensure_default_deck(&pool).await.unwrap();
        let extra_id = insert_deck(&pool, "Spanish").await;
        pool.close().await;

        let pool = open(&path).await.unwrap();
        let default_id = list_decks(&pool)
            .await
            .unwrap()
            .into_iter()
            .find(|d| d.name == DEFAULT_DECK_NAME)
            .unwrap()
            .id;
        assert_eq!(deck_user_id(&pool, default_id).await, None);
        assert_eq!(deck_user_id(&pool, extra_id).await, None);

        pool.close().await;
        remove_db(&path);
    }

    #[tokio::test]
    async fn bootstrap_assigns_orphans_instead_of_second_default() {
        let pool = open_memory().await;
        let preexisting = insert_deck(&pool, "Preexisting").await;
        let store = SqliteStore::new(pool.clone());

        let admin = bootstrap_admin(&store, "admin", "secret").await.unwrap();
        assert!(admin.admin);
        assert_eq!(deck_user_id(&pool, preexisting).await, Some(admin.id));
        let default_id = list_decks(&pool)
            .await
            .unwrap()
            .into_iter()
            .find(|d| d.name == DEFAULT_DECK_NAME)
            .unwrap()
            .id;
        assert_eq!(deck_user_id(&pool, default_id).await, Some(admin.id));

        let home = domain::list_home(&store, admin.id, noon()).await.unwrap();
        assert_eq!(home.len(), 2);
        assert!(home.iter().any(|d| d.deck.name == "Preexisting"));
        assert!(home.iter().any(|d| d.deck.name == DEFAULT_DECK_NAME));
        assert_eq!(
            home.iter()
                .filter(|d| d.deck.name == DEFAULT_DECK_NAME)
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn bootstrap_seeds_default_when_no_orphans() {
        let pool = open_memory_migrations_only().await;
        let store = SqliteStore::new(pool.clone());
        assert!(list_decks(&pool).await.unwrap().is_empty());

        let admin = bootstrap_admin(&store, "admin", "secret").await.unwrap();
        let home = domain::list_home(&store, admin.id, noon()).await.unwrap();
        assert_eq!(home.len(), 1);
        assert_eq!(home[0].deck.name, DEFAULT_DECK_NAME);
        assert_eq!(deck_user_id(&pool, home[0].deck.id).await, Some(admin.id));
    }

    #[tokio::test]
    async fn create_user_seeds_default_per_user() {
        let store = SqliteStore::new(open_memory_migrations_only().await);
        let admin = bootstrap_admin(&store, "admin", "secret").await.unwrap();
        let member = admin_create_user(&store, admin.id, "member", "pw")
            .await
            .unwrap();

        let admin_home = domain::list_home(&store, admin.id, noon()).await.unwrap();
        let member_home = domain::list_home(&store, member.id, noon()).await.unwrap();
        assert_eq!(admin_home.len(), 1);
        assert_eq!(admin_home[0].deck.name, DEFAULT_DECK_NAME);
        assert_eq!(member_home.len(), 1);
        assert_eq!(member_home[0].deck.name, DEFAULT_DECK_NAME);
        assert_ne!(admin_home[0].deck.id, member_home[0].deck.id);
    }

    #[tokio::test]
    async fn store_queries_are_isolated_per_user() {
        let store = SqliteStore::new(open_memory().await);
        let admin = store.create_user("admin", "hash", true).await.unwrap();
        let member = store.create_user("member", "hash", false).await.unwrap();
        let admin_deck = domain::create_deck(&store, admin.id, "AdminDeck")
            .await
            .unwrap();
        let member_deck = domain::create_deck(&store, member.id, "MemberDeck")
            .await
            .unwrap();
        let admin_card = domain::create_card(&store, admin.id, admin_deck.id, "Q", "A")
            .await
            .unwrap();
        let member_card = domain::create_card(&store, member.id, member_deck.id, "Q2", "A2")
            .await
            .unwrap();

        let admin_home = domain::list_home(&store, admin.id, noon()).await.unwrap();
        assert_eq!(admin_home.len(), 1);
        assert_eq!(admin_home[0].deck.name, "AdminDeck");
        assert!(
            domain::get_deck(&store, admin.id, member_deck.id)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            domain::get_card(&store, admin.id, member_card.id)
                .await
                .unwrap()
                .is_none()
        );
        assert!(matches!(
            domain::list_deck_cards(&store, admin.id, member_deck.id)
                .await
                .unwrap_err(),
            domain::Error::DeckNotFound { deck_id } if deck_id == member_deck.id
        ));
        assert!(matches!(
            domain::create_card(&store, admin.id, member_deck.id, "X", "Y")
                .await
                .unwrap_err(),
            domain::Error::DeckNotFound { deck_id } if deck_id == member_deck.id
        ));
        assert!(
            domain::get_deck(&store, member.id, admin_deck.id)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            domain::get_card(&store, member.id, admin_card.id)
                .await
                .unwrap()
                .is_none()
        );

        let now = Utc.with_ymd_and_hms(2026, 9, 13, 12, 0, 0).unwrap();
        let (updated, entry) = member_card.apply_rating(Rating::Good, now).unwrap();
        assert!(matches!(
            store
                .commit_review(admin.id, &updated, &entry)
                .await
                .unwrap_err(),
            domain::Error::CardNotFound { card_id } if card_id == member_card.id
        ));
    }

    #[tokio::test]
    async fn delete_user_cascades_decks_cards_review_logs_and_sessions() {
        let pool = open_memory_migrations_only().await;
        let store = SqliteStore::new(pool.clone());
        let admin = store.create_user("admin", "hash", true).await.unwrap();
        let member = store.create_user("member", "hash", false).await.unwrap();
        let kept = domain::create_deck(&store, admin.id, "Keep").await.unwrap();
        domain::create_card(&store, admin.id, kept.id, "Q", "A")
            .await
            .unwrap();
        let doomed = domain::create_deck(&store, member.id, "Doomed")
            .await
            .unwrap();
        let card = domain::create_card(&store, member.id, doomed.id, "Q", "A")
            .await
            .unwrap();
        let now = Utc.with_ymd_and_hms(2026, 9, 13, 12, 0, 0).unwrap();
        domain::rate(&store, member.id, card.id, Rating::Good, now)
            .await
            .unwrap();
        let session = store.create_session(member.id, now).await.unwrap();

        store.delete_user(member.id).await.unwrap();

        assert!(store.get_user(member.id).await.unwrap().is_none());
        assert!(
            domain::get_deck(&store, admin.id, doomed.id)
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(count(&pool, "sessions").await, 0);
        assert!(store.get_session(&session.id).await.unwrap().is_none());
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM cards WHERE deck_id = ?")
                .bind(doomed.id)
                .fetch_one(&pool)
                .await
                .unwrap(),
            0
        );
        assert_eq!(count(&pool, "review_logs").await, 0);
        assert_eq!(
            domain::list_home(&store, admin.id, now)
                .await
                .unwrap()
                .len(),
            1
        );
        assert!(store.get_user(admin.id).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn disable_keeps_user_row_and_does_not_delete_sessions() {
        let store = SqliteStore::new(open_memory_migrations_only().await);
        let member = store.create_user("member", "hash", false).await.unwrap();
        let now = Utc.with_ymd_and_hms(2026, 9, 13, 12, 0, 0).unwrap();
        let session = store.create_session(member.id, now).await.unwrap();

        let disabled = store.set_disabled(member.id, true).await.unwrap();
        assert!(disabled.disabled);
        assert!(store.get_user(member.id).await.unwrap().unwrap().disabled);
        assert!(store.get_session(&session.id).await.unwrap().is_some());

        store.delete_sessions_for_user(member.id).await.unwrap();
        assert!(store.get_session(&session.id).await.unwrap().is_none());
        assert!(store.get_user(member.id).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn username_is_unique_case_insensitive_and_stored_as_entered() {
        let store = SqliteStore::new(open_memory_migrations_only().await);
        let created = store.create_user("Admin", "hash", true).await.unwrap();
        assert_eq!(created.username, "Admin");
        assert_eq!(
            store
                .get_user_by_username("admin")
                .await
                .unwrap()
                .unwrap()
                .id,
            created.id
        );
        assert!(matches!(
            store.create_user("admin", "hash", false).await.unwrap_err(),
            domain::Error::UsernameTaken
        ));
        assert_eq!(store.list_users().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn session_create_get_and_delete() {
        let store = SqliteStore::new(open_memory_migrations_only().await);
        let user = store.create_user("admin", "hash", true).await.unwrap();
        let now = Utc.with_ymd_and_hms(2026, 9, 13, 12, 0, 0).unwrap();
        let session = store.create_session(user.id, now).await.unwrap();
        assert_eq!(session.user_id, user.id);
        assert_eq!(session.created_at, now);
        assert_eq!(session.last_used_at, now);
        assert_eq!(session.id.len(), 32);

        let fetched = store.get_session(&session.id).await.unwrap().unwrap();
        assert_eq!(fetched, session);

        store.delete_session(&session.id).await.unwrap();
        assert!(store.get_session(&session.id).await.unwrap().is_none());
        store.delete_session("missing").await.unwrap();
    }

    #[tokio::test]
    async fn unknown_user_does_not_see_orphans_once_users_exist() {
        let store = SqliteStore::new(open_memory().await);
        store.create_user("admin", "hash", true).await.unwrap();
        let home = domain::list_home(&store, 999, noon()).await.unwrap();
        assert!(home.is_empty());
        assert!(matches!(
            domain::create_deck(&store, 999, "Nope").await.unwrap_err(),
            domain::Error::UserNotFound { user_id: 999 }
        ));
    }
}
