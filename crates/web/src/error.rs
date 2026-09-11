use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

#[derive(Debug)]
pub enum AppError {
    Db(db::Error),
    Render(askama::Error),
}

impl From<db::Error> for AppError {
    fn from(err: db::Error) -> Self {
        Self::Db(err)
    }
}

impl From<askama::Error> for AppError {
    fn from(err: askama::Error) -> Self {
        Self::Render(err)
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Db(err) => write!(f, "{err}"),
            Self::Render(err) => write!(f, "template error: {err}"),
        }
    }
}

impl std::error::Error for AppError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Db(err) => Some(err),
            Self::Render(err) => Some(err),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        // Log the real cause; never send db/template details to the browser.
        eprintln!("internal error: {self}");
        (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response()
    }
}
