//! Stack-only CI harness: seed one admin + Default into an empty SQLite path
//! via `domain::bootstrap_admin` + `SqliteStore` (not raw SQL). Not product
//! behavior — production `main` stays fail-closed on an empty User table.
//! `e2e/scripts/run-ci.sh` runs this before starting the flashcards binary.
//!
//! Refuses to run unless `CI=true`/`CI=1` or the db path is under the
//! process temp directory (so a production `./data/` path is not seeded).

use std::path::{Path, PathBuf};

use db::SqliteStore;

const USERNAME: &str = "e2e";
const PASSWORD: &str = "e2e-secret";

#[tokio::main]
async fn main() {
    let db_path = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: seed_ci_admin <db-path>");
        std::process::exit(1);
    });

    if !seed_target_allowed(&db_path) {
        eprintln!(
            "seed_ci_admin refuses {db_path}: set CI=true or pass a path under {}",
            std::env::temp_dir().display()
        );
        std::process::exit(1);
    }

    let pool = db::open(&db_path).await.unwrap_or_else(|err| {
        eprintln!("{err}");
        std::process::exit(1);
    });
    let store = SqliteStore::new(pool.clone());
    if let Err(err) = domain::bootstrap_admin(&store, USERNAME, PASSWORD).await {
        eprintln!("{err}");
        pool.close().await;
        std::process::exit(1);
    }
    pool.close().await;
}

fn seed_target_allowed(db_path: &str) -> bool {
    match std::env::var("CI") {
        Ok(value) if value == "true" || value == "1" => return true,
        _ => {}
    }
    path_under_temp(Path::new(db_path))
}

fn path_under_temp(db_path: &Path) -> bool {
    let tmp = std::env::temp_dir();
    let candidate = if db_path.is_absolute() {
        db_path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(db_path))
            .unwrap_or_else(|_| PathBuf::from(db_path))
    };
    let resolved = match candidate.parent() {
        Some(parent) if parent.exists() => parent
            .canonicalize()
            .map(|parent| parent.join(candidate.file_name().unwrap_or_default()))
            .unwrap_or(candidate),
        _ => candidate,
    };
    let tmp = tmp.canonicalize().unwrap_or(tmp);
    resolved.starts_with(&tmp)
}
