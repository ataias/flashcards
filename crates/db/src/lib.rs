//! SQLx models, migrations, and queries.

use std::path::Path;

use chrono::{DateTime, SecondsFormat, TimeZone, Utc};
use domain::{Card, Rating, ScheduleError, ScheduledReview};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{SqlitePool, migrate::Migrator};

/// Deck name created when the database has zero Decks.
pub const DEFAULT_DECK_NAME: &str = "Default";

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deck {
    pub id: i64,
    pub name: String,
}

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
                write!(
                    f,
                    "card {card_id} has incomplete FSRS fields (New Card requires all NULL)"
                )
            }
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
            Self::IncompleteFsrs { .. } => None,
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
    connect(options).await
}

async fn connect(options: SqliteConnectOptions) -> Result<SqlitePool, Error> {
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;
    MIGRATOR.run(&pool).await?;
    ensure_default_deck(&pool).await?;
    Ok(pool)
}

/// COUNT+INSERT run in one `BEGIN IMMEDIATE` transaction so concurrent `open`
/// cannot both observe an empty table and insert a second Default.
pub async fn ensure_default_deck(pool: &SqlitePool) -> Result<(), Error> {
    let mut conn = pool.acquire().await?;
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
    let result = seed_default_if_empty(&mut conn).await;
    finish_immediate(conn, result).await
}

/// Delete a Deck (Cards and Review logs cascade) and recreate `Default` if none remain.
pub async fn delete_deck(pool: &SqlitePool, deck_id: i64) -> Result<(), Error> {
    let mut conn = pool.acquire().await?;
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
    let result = delete_deck_on(&mut conn, deck_id).await;
    finish_immediate(conn, result).await
}

async fn delete_deck_on(
    conn: &mut sqlx::pool::PoolConnection<sqlx::Sqlite>,
    deck_id: i64,
) -> Result<(), Error> {
    sqlx::query("DELETE FROM decks WHERE id = ?")
        .bind(deck_id)
        .execute(&mut **conn)
        .await?;
    seed_default_if_empty(conn).await
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

type CardRow = (
    i64,
    i64,
    String,
    String,
    Option<f64>,
    Option<f64>,
    Option<String>,
    Option<String>,
);

pub async fn get_card(pool: &SqlitePool, card_id: i64) -> Result<Option<Card>, Error> {
    let row = sqlx::query_as::<_, CardRow>(
        "SELECT id, deck_id, front, back, stability, difficulty, due, last_review
         FROM cards WHERE id = ?",
    )
    .bind(card_id)
    .fetch_optional(pool)
    .await?;
    row.map(card_from_row).transpose()
}

pub async fn list_cards_in_deck(pool: &SqlitePool, deck_id: i64) -> Result<Vec<Card>, Error> {
    let rows = sqlx::query_as::<_, CardRow>(
        "SELECT id, deck_id, front, back, stability, difficulty, due, last_review
         FROM cards WHERE deck_id = ? ORDER BY id",
    )
    .bind(deck_id)
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(card_from_row).collect()
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

pub async fn apply_review(
    pool: &SqlitePool,
    card_id: i64,
    scheduled: &ScheduledReview,
    rating: Rating,
) -> Result<(), Error> {
    let mut tx = pool.begin().await?;
    let due = rfc3339(scheduled.due);
    let last_review = rfc3339(scheduled.last_review);
    sqlx::query(
        "UPDATE cards
         SET stability = ?, difficulty = ?, due = ?, last_review = ?
         WHERE id = ?",
    )
    .bind(scheduled.memory.stability)
    .bind(scheduled.memory.difficulty)
    .bind(&due)
    .bind(&last_review)
    .bind(card_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("INSERT INTO review_logs (card_id, rated_at, rating) VALUES (?, ?, ?)")
        .bind(card_id)
        .bind(&last_review)
        .bind(rating.as_grade())
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn rate_card(
    pool: &SqlitePool,
    card: &Card,
    rating: Rating,
    now: DateTime<Utc>,
) -> Result<ScheduledReview, Error> {
    let scheduled = domain::schedule(card.memory, card.last_review, rating, now)?;
    apply_review(pool, card.id, &scheduled, rating).await?;
    Ok(scheduled)
}

fn rfc3339(instant: DateTime<Utc>) -> String {
    instant.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn parse_utc(value: &str) -> Result<DateTime<Utc>, Error> {
    DateTime::parse_from_rfc3339(value)
        .map(|parsed| parsed.with_timezone(&Utc))
        .map_err(|source| Error::InvalidTimestamp {
            value: value.to_string(),
            source,
        })
}

fn card_from_row(row: CardRow) -> Result<Card, Error> {
    let (id, deck_id, front, back, stability, difficulty, due, last_review) = row;
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
    match (&memory, &due, &last_review) {
        (None, None, None) | (Some(_), Some(_), Some(_)) => {}
        _ => return Err(Error::IncompleteFsrs { card_id: id }),
    }
    Ok(Card {
        id,
        deck_id,
        front,
        back,
        memory,
        due,
        last_review,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use domain::{Card, Rating};
    use sqlx::sqlite::SqliteConnectOptions;

    async fn open_memory() -> SqlitePool {
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
    async fn delete_last_deck_recreates_default() {
        let pool = open_memory().await;
        let original = list_decks(&pool).await.unwrap();
        assert_eq!(original.len(), 1);
        let original_id = original[0].id;

        delete_deck(&pool, original_id).await.unwrap();

        let decks = list_decks(&pool).await.unwrap();
        assert_eq!(decks.len(), 1);
        assert_eq!(decks[0].name, DEFAULT_DECK_NAME);
        assert_ne!(decks[0].id, original_id);
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
    async fn delete_deck_cascades_cards_and_review_logs_then_recreates_default() {
        let pool = open_memory().await;
        let deck_id = list_decks(&pool).await.unwrap()[0].id;
        let card_id = insert_card(&pool, deck_id).await;
        insert_review_log(&pool, card_id).await;
        assert_eq!(count(&pool, "cards").await, 1);
        assert_eq!(count(&pool, "review_logs").await, 1);

        delete_deck(&pool, deck_id).await.unwrap();

        assert_eq!(count(&pool, "cards").await, 0);
        assert_eq!(count(&pool, "review_logs").await, 0);
        let decks = list_decks(&pool).await.unwrap();
        assert_eq!(decks.len(), 1);
        assert_eq!(decks[0].name, DEFAULT_DECK_NAME);
        assert_ne!(decks[0].id, deck_id);
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
        let scheduled = rate_card(&pool, &card, Rating::Good, now).await.unwrap();

        let updated = get_card(&pool, card_id).await.unwrap().unwrap();
        assert!(!updated.is_new());
        assert_eq!(updated.memory, Some(scheduled.memory));
        assert_eq!(updated.due, Some(scheduled.due));
        assert_eq!(updated.last_review, Some(now));
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
}
