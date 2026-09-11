use axum::Router;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    let app = Router::new();
    let listener = TcpListener::bind("127.0.0.1:3000")
        .await
        .expect("failed to bind 127.0.0.1:3000");
    axum::serve(listener, app).await.expect("server error");
}
