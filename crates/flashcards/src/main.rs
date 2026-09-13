use db::SqliteStore;
use domain::Store;
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
    let user_id = match store.list_users().await {
        Ok(users) => match users.into_iter().next() {
            Some(user) => user.id,
            None => match bootstrap_admin_from_env(&store).await {
                Ok(user) => user.id,
                Err(err) => {
                    eprintln!("{err}");
                    std::process::exit(1);
                }
            },
        },
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(1);
        }
    };
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
    let result = axum::serve(listener, web::app(store, user_id)).await;
    pool.close().await;
    if let Err(err) = result {
        eprintln!("server error: {err}");
        std::process::exit(1);
    }
}

/// Create the first admin only when a password is supplied. No default.
async fn bootstrap_admin_from_env(store: &SqliteStore) -> Result<domain::User, String> {
    let password = std::env::var("FLASHCARDS_BOOTSTRAP_ADMIN_PASSWORD")
        .ok()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            "no users; set FLASHCARDS_BOOTSTRAP_ADMIN_PASSWORD to create the first admin, or use a database that already has one".to_string()
        })?;
    let username = std::env::var("FLASHCARDS_BOOTSTRAP_ADMIN_USERNAME")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "admin".to_string());
    domain::bootstrap_admin(store, &username, &password)
        .await
        .map_err(|err| err.to_string())
}
