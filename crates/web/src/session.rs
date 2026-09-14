use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::Extension;
use axum::body::{Body, to_bytes};
use axum::extract::{FromRequestParts, State};
use axum::http::header::{COOKIE, SET_COOKIE};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::{extract::Request, middleware::Next};
use chrono::Utc;
use domain::{Session, Store, User};
use rand_core::{OsRng, RngCore};

use crate::error::AppError;
use crate::wants_fragment;

pub const SESSION_COOKIE: &str = "session";
pub const CSRF_COOKIE: &str = "csrf";
pub const CSRF_FIELD: &str = "csrf";

const COOKIE_MAX_AGE: i64 = 30 * 24 * 60 * 60;
const LOGIN_RATE_MAX: usize = 5;
const LOGIN_RATE_WINDOW: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone, Copy)]
pub struct CookieSecure(pub bool);

#[derive(Debug, Clone)]
pub struct CsrfToken(pub String);

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user: User,
    /// Live Session when a cookie was presented; empty id for the
    /// seed/e2e implicit User until the e2e child removes that gate.
    pub session: Session,
}

#[derive(Clone)]
pub struct RateLimiter {
    inner: Arc<Mutex<HashMap<String, Vec<Instant>>>>,
    max: usize,
    window: Duration,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new(LOGIN_RATE_MAX, LOGIN_RATE_WINDOW)
    }
}

impl RateLimiter {
    pub fn new(max: usize, window: Duration) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            max,
            window,
        }
    }

    /// Record an attempt. `false` when the key is already at the limit.
    pub fn allow(&self, key: &str) -> bool {
        let mut map = self.inner.lock().expect("rate limiter lock");
        let now = Instant::now();
        let entries = map.entry(key.to_string()).or_default();
        entries.retain(|at| now.duration_since(*at) < self.window);
        if entries.len() >= self.max {
            return false;
        }
        entries.push(now);
        true
    }
}

impl<S> FromRequestParts<S> for CsrfToken
where
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts.extensions.get::<CsrfToken>().cloned().ok_or_else(|| {
            (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response()
        })
    }
}

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<AuthUser>()
            .cloned()
            .ok_or_else(|| Redirect::to("/login").into_response())
    }
}

pub fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    let cookie = headers.get(COOKIE)?.to_str().ok()?;
    cookie.split(';').find_map(|part| {
        let part = part.trim();
        let (key, value) = part.split_once('=')?;
        (key.trim() == name).then(|| value.trim().to_string())
    })
}

pub fn client_key(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("local")
        .to_string()
}

pub fn set_session_cookie(headers: &mut HeaderMap, session_id: &str, secure: bool) {
    append_cookie(headers, SESSION_COOKIE, session_id, COOKIE_MAX_AGE, secure);
}

pub fn clear_session_cookie(headers: &mut HeaderMap, secure: bool) {
    append_cookie(headers, SESSION_COOKIE, "", 0, secure);
}

pub fn rate_limited() -> Response {
    (StatusCode::TOO_MANY_REQUESTS, "Too many attempts").into_response()
}

pub async fn load_auth<S: Store>(
    store: &S,
    headers: &HeaderMap,
) -> Result<Option<AuthUser>, domain::Error> {
    let Some(session_id) = cookie_value(headers, SESSION_COOKIE) else {
        return Ok(None);
    };
    Ok(domain::resolve_session(store, &session_id, Utc::now())
        .await?
        .map(|(user, session)| AuthUser { user, session }))
}

pub async fn require_auth<S: Store>(
    State(store): State<S>,
    mut req: Request,
    next: Next,
) -> Response {
    match load_auth(&store, req.headers()).await {
        Ok(Some(auth)) => {
            req.extensions_mut().insert(auth);
            next.run(req).await
        }
        Ok(None) => match implicit_or_gate(&store).await {
            Ok(Some(auth)) => {
                req.extensions_mut().insert(auth);
                next.run(req).await
            }
            Ok(None) => redirect_to_sign_in(req.headers(), "/bootstrap"),
            Err(err) => AppError::from(err).into_response(),
        },
        Err(err) => AppError::from(err).into_response(),
    }
}

/// Seed/e2e compat until login UI lands: if Users exist, treat the first
/// non-disabled User as signed in when no Session cookie is present.
async fn implicit_or_gate<S: Store>(store: &S) -> Result<Option<AuthUser>, domain::Error> {
    let users = store.list_users().await?;
    if users.is_empty() {
        return Ok(None);
    }
    let Some(user) = users.into_iter().find(|user| !user.disabled) else {
        return Ok(None);
    };
    let now = Utc::now();
    Ok(Some(AuthUser {
        user: user.clone(),
        session: Session {
            id: String::new(),
            user_id: user.id,
            created_at: now,
            last_used_at: now,
        },
    }))
}

pub async fn csrf_middleware(
    Extension(CookieSecure(secure)): Extension<CookieSecure>,
    req: Request,
    next: Next,
) -> Response {
    if req.uri().path().starts_with("/static") {
        return next.run(req).await;
    }

    let existing = cookie_value(req.headers(), CSRF_COOKIE);
    let token = existing.clone().unwrap_or_else(random_token);
    let mutating = matches!(
        *req.method(),
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    );

    let (mut parts, body) = req.into_parts();
    let bytes = match to_bytes(body, usize::MAX).await {
        Ok(bytes) => bytes,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };

    if mutating {
        let header = parts
            .headers
            .get("x-csrf-token")
            .and_then(|value| value.to_str().ok());
        let form = form_field(std::str::from_utf8(&bytes).unwrap_or(""), CSRF_FIELD);
        let submitted = header.or(form.as_deref());
        if submitted != Some(token.as_str()) {
            return (StatusCode::FORBIDDEN, "Invalid CSRF token").into_response();
        }
    }

    parts.extensions.insert(CsrfToken(token.clone()));
    let req = Request::from_parts(parts, Body::from(bytes));
    let mut response = next.run(req).await;
    if existing.is_none() {
        append_cookie(
            response.headers_mut(),
            CSRF_COOKIE,
            &token,
            COOKIE_MAX_AGE,
            secure,
        );
    }
    response
}

pub fn redirect_to_sign_in(headers: &HeaderMap, dest: &str) -> Response {
    let mut response = Redirect::to(dest).into_response();
    if wants_fragment(headers)
        && let Ok(value) = HeaderValue::from_str(dest)
    {
        response.headers_mut().insert("HX-Redirect", value);
    }
    response
}

fn append_cookie(headers: &mut HeaderMap, name: &str, value: &str, max_age: i64, secure: bool) {
    let secure_attr = if secure { "; Secure" } else { "" };
    let cookie = format!(
        "{name}={value}; Path=/; HttpOnly{secure_attr}; SameSite=Strict; Max-Age={max_age}"
    );
    headers.append(
        SET_COOKIE,
        HeaderValue::from_str(&cookie).expect("cookie is ascii"),
    );
}

fn random_token() -> String {
    let mut bytes = [0u8; 16];
    OsRng.fill_bytes(&mut bytes);
    bytes
        .iter()
        .fold(String::with_capacity(32), |mut out, byte| {
            use std::fmt::Write;
            let _ = write!(out, "{byte:02x}");
            out
        })
}

fn form_field(body: &str, name: &str) -> Option<String> {
    body.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == name).then(|| value.replace('+', " "))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;

    #[test]
    fn cookie_value_reads_named_pair() {
        let mut headers = HeaderMap::new();
        headers.insert(COOKIE, HeaderValue::from_static("csrf=abc; session=xyz"));
        assert_eq!(
            cookie_value(&headers, SESSION_COOKIE).as_deref(),
            Some("xyz")
        );
        assert_eq!(cookie_value(&headers, CSRF_COOKIE).as_deref(), Some("abc"));
        assert_eq!(cookie_value(&headers, "missing"), None);
    }

    #[test]
    fn rate_limiter_trips_after_max() {
        let limiter = RateLimiter::new(2, Duration::from_secs(60));
        assert!(limiter.allow("local") && limiter.allow("local"));
        assert!(!limiter.allow("local"));
        assert!(limiter.allow("other"));
    }

    #[test]
    fn session_cookie_is_http_only_secure_strict() {
        let mut headers = HeaderMap::new();
        set_session_cookie(&mut headers, "deadbeef", true);
        let value = headers[SET_COOKIE].to_str().unwrap();
        assert!(value.contains("session=deadbeef"));
        assert!(value.contains("HttpOnly"));
        assert!(value.contains("Secure"));
        assert!(value.contains("SameSite=Strict"));
        assert!(value.contains("Path=/"));

        let mut headers = HeaderMap::new();
        set_session_cookie(&mut headers, "deadbeef", false);
        let value = headers[SET_COOKIE].to_str().unwrap();
        assert!(value.contains("HttpOnly") && value.contains("SameSite=Strict"));
        assert!(!value.contains("Secure"));
    }
}
