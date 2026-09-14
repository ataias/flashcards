use axum::Extension;
use axum::extract::{Form, State};
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use chrono::Utc;
use domain::Store;
use serde::Deserialize;

use crate::error::AppError;
use crate::session::{CookieSecure, RateLimiter, client_key, rate_limited, set_session_cookie};

#[derive(Deserialize)]
pub struct LoginForm {
    username: String,
    password: String,
}

/// POST-only login (no page yet). Used for session cookies and rate-limit tests.
pub async fn login<S: Store>(
    State(store): State<S>,
    Extension(limiter): Extension<RateLimiter>,
    Extension(CookieSecure(secure)): Extension<CookieSecure>,
    headers: HeaderMap,
    Form(form): Form<LoginForm>,
) -> Result<Response, AppError> {
    if !limiter.allow(&client_key(&headers)) {
        return Ok(rate_limited());
    }
    if store.list_users().await?.is_empty() {
        return Ok(Redirect::to("/bootstrap").into_response());
    }
    match domain::authenticate(&store, &form.username, &form.password, Utc::now()).await {
        Ok((_, session)) => {
            let mut response = Redirect::to("/").into_response();
            set_session_cookie(response.headers_mut(), &session.id, secure);
            Ok(response)
        }
        Err(domain::Error::InvalidCredentials) => {
            Ok((StatusCode::UNAUTHORIZED, "Invalid username or password").into_response())
        }
        Err(err) => Err(err.into()),
    }
}
