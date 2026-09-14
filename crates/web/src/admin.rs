use askama::Template;
use axum::extract::{Form, Path, State};
use axum::response::{Html, IntoResponse, Redirect, Response};
use domain::{Store, User};
use serde::Deserialize;

use crate::assets::Head;
use crate::error::AppError;
use crate::session::{AdminUser, AuthUser, CsrfToken};

#[derive(Template)]
#[template(path = "admin_users.html")]
struct AdminUsersTemplate {
    csrf: String,
    username: String,
    admin: bool,
    users: Vec<UserRow>,
    error: Option<String>,
    head: Head,
}

struct UserRow {
    id: i64,
    username: String,
    is_admin: bool,
    disabled: bool,
    is_self: bool,
}

#[derive(Deserialize)]
pub struct CreateUserForm {
    username: String,
    password: String,
}

#[derive(Deserialize)]
pub struct ResetPasswordForm {
    password: String,
}

pub async fn users_page<S: Store>(
    State(store): State<S>,
    admin: AdminUser,
    csrf: CsrfToken,
) -> Result<Response, AppError> {
    render_users(&store, &admin.0, &csrf.0, None).await
}

pub async fn create_user<S: Store>(
    State(store): State<S>,
    admin: AdminUser,
    csrf: CsrfToken,
    Form(form): Form<CreateUserForm>,
) -> Result<Response, AppError> {
    match domain::admin_create_user(&store, admin.0.user.id, &form.username, &form.password).await {
        Ok(_) => Ok(Redirect::to("/admin/users").into_response()),
        Err(err) => render_admin_error(&store, &admin.0, &csrf.0, err).await,
    }
}

pub async fn disable_user<S: Store>(
    State(store): State<S>,
    Path(user_id): Path<i64>,
    admin: AdminUser,
    csrf: CsrfToken,
) -> Result<Response, AppError> {
    match domain::admin_disable_user(&store, admin.0.user.id, user_id).await {
        Ok(_) => Ok(Redirect::to("/admin/users").into_response()),
        Err(err) => render_admin_error(&store, &admin.0, &csrf.0, err).await,
    }
}

pub async fn delete_user<S: Store>(
    State(store): State<S>,
    Path(user_id): Path<i64>,
    admin: AdminUser,
    csrf: CsrfToken,
) -> Result<Response, AppError> {
    match domain::admin_delete_user(&store, admin.0.user.id, user_id).await {
        Ok(()) => Ok(Redirect::to("/admin/users").into_response()),
        Err(err) => render_admin_error(&store, &admin.0, &csrf.0, err).await,
    }
}

pub async fn reset_password<S: Store>(
    State(store): State<S>,
    Path(user_id): Path<i64>,
    admin: AdminUser,
    csrf: CsrfToken,
    Form(form): Form<ResetPasswordForm>,
) -> Result<Response, AppError> {
    match domain::admin_reset_password(&store, admin.0.user.id, user_id, &form.password).await {
        Ok(_) => Ok(Redirect::to("/admin/users").into_response()),
        Err(err) => render_admin_error(&store, &admin.0, &csrf.0, err).await,
    }
}

async fn render_admin_error<S: Store>(
    store: &S,
    auth: &AuthUser,
    csrf: &str,
    err: domain::Error,
) -> Result<Response, AppError> {
    let message = match err {
        domain::Error::InvalidUsername => {
            "Username must be 3–32 characters and use only A–Z, a–z, 0–9, _, ., or -."
        }
        domain::Error::UsernameTaken => "Username is already taken.",
        domain::Error::EmptyPassword => "Password cannot be empty.",
        domain::Error::CannotModifySelf => "You cannot disable, delete, or reset your own account.",
        domain::Error::CannotRemoveLastAdmin => "You cannot disable or delete the last admin.",
        domain::Error::UserDisabled => "That User is disabled.",
        domain::Error::UserNotFound { .. } => {
            return Err(err.into());
        }
        other => return Err(other.into()),
    };
    render_users(store, auth, csrf, Some(message)).await
}

async fn render_users<S: Store>(
    store: &S,
    auth: &AuthUser,
    csrf: &str,
    error: Option<&str>,
) -> Result<Response, AppError> {
    let users = store
        .list_users()
        .await?
        .into_iter()
        .map(|user| user_row(&user, auth.user.id))
        .collect();
    Ok(Html(
        AdminUsersTemplate {
            csrf: csrf.to_string(),
            username: auth.user.username.clone(),
            admin: auth.user.admin,
            users,
            error: error.map(str::to_string),
            head: Head::new("Users — Flashcards")?,
        }
        .render()?,
    )
    .into_response())
}

fn user_row(user: &User, actor_id: domain::UserId) -> UserRow {
    UserRow {
        id: user.id,
        username: user.username.clone(),
        is_admin: user.admin,
        disabled: user.disabled,
        is_self: user.id == actor_id,
    }
}
