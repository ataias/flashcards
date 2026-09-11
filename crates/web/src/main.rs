use tokio::net::TcpListener;
use web::Config;

#[tokio::main]
async fn main() {
    let config = Config::from_env().unwrap_or_else(|err| {
        eprintln!("{err}");
        std::process::exit(1);
    });

    eprintln!("listening on http://{}", config.bind);
    eprintln!("FLASHCARDS_DB={}", config.db_path.display());

    let listener = TcpListener::bind(config.bind)
        .await
        .unwrap_or_else(|err| panic!("failed to bind {}: {err}", config.bind));
    axum::serve(listener, web::app())
        .await
        .expect("server error");
}
