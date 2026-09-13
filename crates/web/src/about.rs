use askama::Template;
use axum::response::{Html, IntoResponse, Response};

use crate::build_info::{
    VERSION, commit_url, git_sha, git_tag, release_label, release_url, short_sha,
};
use crate::error::AppError;

#[derive(Template)]
#[template(path = "about.html")]
struct AboutTemplate<'a> {
    version: &'a str,
    short_sha: &'a str,
    commit_url: Option<String>,
    release_url: String,
    release_label: String,
}

pub async fn about() -> Result<Response, AppError> {
    let sha = git_sha();
    let tag = git_tag();
    Ok(Html(
        AboutTemplate {
            version: VERSION,
            short_sha: short_sha(sha),
            commit_url: commit_url(sha),
            release_url: release_url(tag),
            release_label: release_label(tag, VERSION),
        }
        .render()?,
    )
    .into_response())
}
