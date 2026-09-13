use db::SqliteStore;
use tokio::net::TcpListener;
use web::Config;

#[tokio::main]
async fn main() {
    let config = Config::from_env().unwrap_or_else(|err| {
        eprintln!("{err}");
        std::process::exit(1);
    });

    let pool = db::open(&config.db_path).await.unwrap_or_else(|err| {
        eprintln!("{err}");
        std::process::exit(1);
    });
    let store = SqliteStore::new(pool.clone());
    let decks = db::list_decks(&pool).await.unwrap_or_else(|err| {
        eprintln!("{err}");
        std::process::exit(1);
    });

    eprintln!("listening on http://{}", config.bind);
    eprintln!(
        "FLASHCARDS_DB={} ({} deck{})",
        config.db_path.display(),
        decks.len(),
        if decks.len() == 1 { "" } else { "s" }
    );

    let listener = TcpListener::bind(config.bind).await.unwrap_or_else(|err| {
        eprintln!("failed to bind {}: {err}", config.bind);
        std::process::exit(1);
    });
    let result = axum::serve(listener, web::app(store, config.cookie_secure)).await;
    pool.close().await;
    if let Err(err) = result {
        eprintln!("server error: {err}");
        std::process::exit(1);
    }
}
