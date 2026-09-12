use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

#[derive(Debug)]
pub enum AppError {
    Domain(domain::Error),
    Render(askama::Error),
}

impl From<domain::Error> for AppError {
    fn from(err: domain::Error) -> Self {
        Self::Domain(err)
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
            Self::Domain(err) => write!(f, "{err}"),
            Self::Render(err) => write!(f, "template error: {err}"),
        }
    }
}

impl std::error::Error for AppError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Domain(err) => Some(err),
            Self::Render(err) => Some(err),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            Self::Domain(
                domain::Error::DeckNotFound { .. } | domain::Error::CardNotFound { .. },
            ) => StatusCode::NOT_FOUND.into_response(),
            Self::Domain(
                domain::Error::EmptyDeckName
                | domain::Error::EmptyCardFront
                | domain::Error::EmptyCardBack,
            ) => (StatusCode::BAD_REQUEST, "Bad request").into_response(),
            err @ (Self::Domain(domain::Error::Schedule(_))
            | Self::Domain(domain::Error::Storage(_))
            | Self::Render(_)) => {
                eprintln!("internal error: {err}");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response()
            }
        }
    }
}
