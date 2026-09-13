//! Stack-only CI harness: seed one admin + Default into an empty SQLite path
//! via `domain::bootstrap_admin` + `SqliteStore` (not raw SQL). Not product
//! behavior — production `main` stays fail-closed on an empty User table.
//! `e2e/scripts/run-ci.sh` runs this before starting the flashcards binary.

use db::SqliteStore;

const USERNAME: &str = "e2e";
const PASSWORD: &str = "e2e-secret";

#[tokio::main]
async fn main() {
    let db_path = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: seed_ci_admin <db-path>");
        std::process::exit(1);
    });

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
