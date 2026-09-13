use askama::Template;
use axum::Extension;
use axum::extract::{Form, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Redirect, Response};
use chrono::Utc;
use domain::Store;
use serde::Deserialize;

use crate::assets::Head;
use crate::error::AppError;
use crate::session::{
    AuthUser, CookieSecure, CsrfToken, RateLimiter, clear_session_cookie, client_key, load_auth,
    rate_limited, set_session_cookie,
};

#[derive(Template)]
#[template(path = "login.html")]
struct LoginTemplate {
    csrf: String,
    error: Option<String>,
    head: Head,
}

#[derive(Template)]
#[template(path = "bootstrap.html")]
struct BootstrapTemplate {
    csrf: String,
    error: Option<String>,
    head: Head,
}

#[derive(Template)]
#[template(path = "settings.html")]
struct SettingsTemplate {
    csrf: String,
    username: String,
    admin: bool,
    error: Option<String>,
    head: Head,
}

#[derive(Deserialize)]
pub struct LoginForm {
    username: String,
    password: String,
}

#[derive(Deserialize)]
pub struct PasswordForm {
    current: String,
    new_password: String,
}

pub async fn login_page<S: Store>(
    State(store): State<S>,
    headers: HeaderMap,
    csrf: CsrfToken,
) -> Result<Response, AppError> {
    if load_auth(&store, &headers).await?.is_some() {
        return Ok(Redirect::to("/").into_response());
    }
    if store.list_users().await?.is_empty() {
        return Ok(Redirect::to("/bootstrap").into_response());
    }
    render_login(&csrf.0, None)
}

pub async fn login<S: Store>(
    State(store): State<S>,
    Extension(limiter): Extension<RateLimiter>,
    Extension(CookieSecure(secure)): Extension<CookieSecure>,
    headers: HeaderMap,
    csrf: CsrfToken,
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
            render_login(&csrf.0, Some("Invalid username or password."))
        }
        Err(err) => Err(err.into()),
    }
}

pub async fn bootstrap_page<S: Store>(
    State(store): State<S>,
    headers: HeaderMap,
    csrf: CsrfToken,
) -> Result<Response, AppError> {
    if load_auth(&store, &headers).await?.is_some() {
        return Ok(Redirect::to("/").into_response());
    }
    if !store.list_users().await?.is_empty() {
        return Ok(Redirect::to("/login").into_response());
    }
    render_bootstrap(&csrf.0, None)
}

pub async fn bootstrap<S: Store>(
    State(store): State<S>,
    Extension(limiter): Extension<RateLimiter>,
    headers: HeaderMap,
    csrf: CsrfToken,
    Form(form): Form<LoginForm>,
) -> Result<Response, AppError> {
    if !limiter.allow(&client_key(&headers)) {
        return Ok(rate_limited());
    }
    match domain::bootstrap_admin(&store, &form.username, &form.password).await {
        Ok(_) => Ok(Redirect::to("/login").into_response()),
        Err(domain::Error::BootstrapNotAllowed) => Ok(Redirect::to("/login").into_response()),
        Err(domain::Error::InvalidUsername) => render_bootstrap(
            &csrf.0,
            Some("Username must be 3–32 characters and use only A–Z, a–z, 0–9, _, ., or -."),
        ),
        Err(domain::Error::EmptyPassword) => {
            render_bootstrap(&csrf.0, Some("Password cannot be empty."))
        }
        Err(err) => Err(err.into()),
    }
}

pub async fn logout<S: Store>(
    State(store): State<S>,
    Extension(CookieSecure(secure)): Extension<CookieSecure>,
    auth: AuthUser,
) -> Result<Response, AppError> {
    domain::logout(&store, &auth.session.id).await?;
    let mut response = Redirect::to("/login").into_response();
    clear_session_cookie(response.headers_mut(), secure);
    Ok(response)
}

pub async fn logout_everywhere<S: Store>(
    State(store): State<S>,
    Extension(CookieSecure(secure)): Extension<CookieSecure>,
    auth: AuthUser,
) -> Result<Response, AppError> {
    domain::logout_everywhere(&store, auth.user.id).await?;
    let mut response = Redirect::to("/login").into_response();
    clear_session_cookie(response.headers_mut(), secure);
    Ok(response)
}

pub async fn settings_page(auth: AuthUser, csrf: CsrfToken) -> Result<Response, AppError> {
    render_settings(&auth, &csrf.0, None)
}

pub async fn change_password<S: Store>(
    State(store): State<S>,
    Extension(CookieSecure(secure)): Extension<CookieSecure>,
    auth: AuthUser,
    csrf: CsrfToken,
    Form(form): Form<PasswordForm>,
) -> Result<Response, AppError> {
    match domain::change_password(&store, auth.user.id, &form.current, &form.new_password).await {
        Ok(()) => {
            let mut response = Redirect::to("/login").into_response();
            clear_session_cookie(response.headers_mut(), secure);
            Ok(response)
        }
        Err(domain::Error::InvalidCredentials) => {
            render_settings(&auth, &csrf.0, Some("Current password is wrong."))
        }
        Err(domain::Error::EmptyPassword) => {
            render_settings(&auth, &csrf.0, Some("Password cannot be empty."))
        }
        Err(err) => Err(err.into()),
    }
}

fn render_login(csrf: &str, error: Option<&str>) -> Result<Response, AppError> {
    Ok(Html(
        LoginTemplate {
            csrf: csrf.to_string(),
            error: error.map(str::to_string),
            head: Head::new("Log in — Flashcards")?,
        }
        .render()?,
    )
    .into_response())
}

fn render_bootstrap(csrf: &str, error: Option<&str>) -> Result<Response, AppError> {
    Ok(Html(
        BootstrapTemplate {
            csrf: csrf.to_string(),
            error: error.map(str::to_string),
            head: Head::new("Create admin — Flashcards")?,
        }
        .render()?,
    )
    .into_response())
}

fn render_settings(auth: &AuthUser, csrf: &str, error: Option<&str>) -> Result<Response, AppError> {
    Ok(Html(
        SettingsTemplate {
            csrf: csrf.to_string(),
            username: auth.user.username.clone(),
            admin: auth.user.admin,
            error: error.map(str::to_string),
            head: Head::new("Settings — Flashcards")?,
        }
        .render()?,
    )
    .into_response())
}
