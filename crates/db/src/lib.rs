//! SQLx models, migrations, and queries.

use std::path::Path;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{migrate::Migrator, SqlitePool};

/// Deck name created when the database has zero Decks (SPEC / CONTEXT).
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
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::CreateParent { source, .. } => Some(source),
            Self::Sqlx(err) => Some(err),
            Self::Migrate(err) => Some(err),
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

/// Open (or create) the SQLite file, run migrations, and seed `Default` if needed.
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

/// Insert Deck named `Default` when zero Decks remain.
pub async fn ensure_default_deck(pool: &SqlitePool) -> Result<(), Error> {
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM decks")
        .fetch_one(pool)
        .await?;
    if n == 0 {
        sqlx::query("INSERT INTO decks (name) VALUES (?)")
            .bind(DEFAULT_DECK_NAME)
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// Delete a Deck (Cards and Review logs cascade) and recreate `Default` if none remain.
pub async fn delete_deck(pool: &SqlitePool, deck_id: i64) -> Result<(), Error> {
    sqlx::query("DELETE FROM decks WHERE id = ?")
        .bind(deck_id)
        .execute(pool)
        .await?;
    ensure_default_deck(pool).await?;
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
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
}
