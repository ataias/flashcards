use axum::Extension;
use axum::extract::{Form, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use chrono::Utc;
use domain::Store;
use serde::Deserialize;

use crate::error::AppError;
use crate::session::{CookieSecure, set_session_cookie};

#[derive(Deserialize)]
pub struct LoginForm {
    username: String,
    password: String,
}

/// POST-only login (no page yet). Used to set a Session cookie in HTTP tests.
pub async fn login<S: Store>(
    State(store): State<S>,
    Extension(CookieSecure(secure)): Extension<CookieSecure>,
    Form(form): Form<LoginForm>,
) -> Result<Response, AppError> {
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
