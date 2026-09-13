use axum::extract::{FromRequestParts, State};
use axum::http::header::{COOKIE, SET_COOKIE};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::{extract::Request, middleware::Next};
use chrono::Utc;
use domain::{Session, Store, User};

use crate::error::AppError;
use crate::wants_fragment;

pub const SESSION_COOKIE: &str = "session";

const COOKIE_MAX_AGE: i64 = 30 * 24 * 60 * 60;

#[derive(Debug, Clone, Copy)]
pub struct CookieSecure(pub bool);

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user: User,
    /// Live Session when a cookie was presented; placeholder for the
    /// seed/e2e implicit User. Logout in the UI follow-up reads this.
    #[allow(dead_code)]
    pub session: Session,
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

pub fn set_session_cookie(headers: &mut HeaderMap, session_id: &str, secure: bool) {
    append_cookie(headers, SESSION_COOKIE, session_id, COOKIE_MAX_AGE, secure);
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
        assert_eq!(cookie_value(&headers, "missing"), None);
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
    }
}
