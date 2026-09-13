use askama::Template;
use axum::response::{Html, IntoResponse, Response};

use crate::build_info::{
    VERSION, commit_url, git_sha, git_tag, image_size_label, release_label, release_url, short_sha,
    uncompressed_image_size_bytes,
};
use crate::error::AppError;

#[derive(Template)]
#[template(path = "about.html")]
struct AboutTemplate {
    version: &'static str,
    short_sha: String,
    commit_url: Option<String>,
    release_url: String,
    release_label: String,
    image_size: String,
}

pub async fn about() -> Result<Response, AppError> {
    let sha = git_sha();
    let tag = git_tag();
    Ok(Html(
        AboutTemplate {
            version: VERSION,
            short_sha: short_sha(&sha).to_string(),
            commit_url: commit_url(&sha),
            release_url: release_url(tag.as_deref()),
            release_label: release_label(tag.as_deref(), VERSION),
            image_size: image_size_label(uncompressed_image_size_bytes()),
        }
        .render()?,
    )
    .into_response())
}
