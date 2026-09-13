pub const REPO_URL: &str = "https://github.com/ataias/flashcards";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn git_sha() -> &'static str {
    nonempty(option_env!("FLASHCARDS_GIT_SHA"))
        .or_else(|| nonempty(option_env!("GITHUB_SHA")))
        .unwrap_or("unknown")
}

pub fn git_tag() -> Option<&'static str> {
    nonempty(option_env!("FLASHCARDS_GIT_TAG"))
}

fn nonempty(value: Option<&'static str>) -> Option<&'static str> {
    value.filter(|value| !value.is_empty())
}

pub fn short_sha(sha: &str) -> &str {
    const SHORT: usize = 7;
    if sha == "unknown" || sha.len() <= SHORT {
        sha
    } else {
        &sha[..SHORT]
    }
}

pub fn commit_url(sha: &str) -> Option<String> {
    (sha != "unknown").then(|| format!("{REPO_URL}/commit/{sha}"))
}

pub fn release_url(tag: Option<&str>) -> String {
    match tag {
        Some(tag) => format!("{REPO_URL}/releases/tag/{tag}"),
        None => format!("{REPO_URL}/releases"),
    }
}

pub fn release_label(tag: Option<&str>, version: &str) -> String {
    match tag {
        Some(tag) => tag.to_string(),
        None => version.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{REPO_URL, VERSION, commit_url, release_label, release_url, short_sha};

    #[test]
    fn short_sha_truncates_full_hex() {
        assert_eq!(
            short_sha("0123456789abcdef0123456789abcdef01234567"),
            "0123456"
        );
        assert_eq!(short_sha("abc"), "abc");
        assert_eq!(short_sha("unknown"), "unknown");
    }

    #[test]
    fn commit_url_skips_unknown() {
        assert_eq!(commit_url("unknown"), None);
        assert_eq!(
            commit_url("0123456789abcdef0123456789abcdef01234567"),
            Some(format!(
                "{REPO_URL}/commit/0123456789abcdef0123456789abcdef01234567"
            ))
        );
    }

    #[test]
    fn release_falls_back_to_index_and_package_version() {
        assert_eq!(release_url(None), format!("{REPO_URL}/releases"));
        assert_eq!(release_label(None, VERSION), VERSION);
        assert_eq!(
            release_url(Some("v0.1.0")),
            format!("{REPO_URL}/releases/tag/v0.1.0")
        );
        assert_eq!(release_label(Some("v0.1.0"), VERSION), "v0.1.0");
    }
}
