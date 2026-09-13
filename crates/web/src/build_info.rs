pub const REPO_URL: &str = "https://github.com/ataias/flashcards";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn git_sha() -> &'static str {
    let sha = env!("FLASHCARDS_GIT_SHA");
    if valid_git_sha(sha) { sha } else { "unknown" }
}

pub fn git_tag() -> Option<&'static str> {
    option_env!("FLASHCARDS_GIT_TAG").filter(|tag| valid_git_tag(tag))
}

fn valid_git_sha(sha: &str) -> bool {
    let len = sha.len();
    (7..=64).contains(&len) && sha.bytes().all(|b| b.is_ascii_hexdigit())
}

fn valid_git_tag(tag: &str) -> bool {
    let bytes = tag.as_bytes();
    !bytes.is_empty()
        && bytes[0] != b'/'
        && bytes.last() != Some(&b'/')
        && !tag.contains("..")
        && bytes
            .iter()
            .all(|&b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'/'))
}

pub fn short_sha(sha: &str) -> &str {
    const SHORT: usize = 7;
    if !valid_git_sha(sha) {
        return "unknown";
    }
    if sha.len() <= SHORT {
        sha
    } else {
        &sha[..SHORT]
    }
}

pub fn commit_url(sha: &str) -> Option<String> {
    valid_git_sha(sha).then(|| format!("{REPO_URL}/commit/{sha}"))
}

pub fn release_url(tag: Option<&str>) -> String {
    match tag.filter(|tag| valid_git_tag(tag)) {
        Some(tag) => format!(
            "{REPO_URL}/releases/tag/{encoded}",
            encoded = encode_tag(tag)
        ),
        None => format!("{REPO_URL}/releases"),
    }
}

pub fn release_label(tag: Option<&str>, version: &str) -> String {
    match tag.filter(|tag| valid_git_tag(tag)) {
        Some(tag) => tag.to_string(),
        None => version.to_string(),
    }
}

fn encode_tag(tag: &str) -> String {
    let mut out = String::with_capacity(tag.len());
    for &b in tag.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                out.push(b as char);
            }
            _ => {
                out.push('%');
                out.push(hex_digit(b >> 4));
                out.push(hex_digit(b & 0x0f));
            }
        }
    }
    out
}

fn hex_digit(n: u8) -> char {
    char::from(if n < 10 { b'0' + n } else { b'A' + (n - 10) })
}

#[cfg(test)]
mod tests {
    use super::{
        REPO_URL, VERSION, commit_url, encode_tag, git_sha, release_label, release_url, short_sha,
        valid_git_sha, valid_git_tag,
    };

    #[test]
    fn baked_sha_is_unknown_or_hex() {
        let sha = git_sha();
        assert!(sha == "unknown" || valid_git_sha(sha), "{sha}");
    }

    #[test]
    fn short_sha_requires_hex_and_truncates() {
        assert_eq!(
            short_sha("0123456789abcdef0123456789abcdef01234567"),
            "0123456"
        );
        assert_eq!(short_sha("abcdef0"), "abcdef0");
        assert_eq!(short_sha("abc"), "unknown");
        assert_eq!(short_sha("unknown"), "unknown");
        assert_eq!(short_sha("../etc/passwd"), "unknown");
        assert_eq!(short_sha("zzzzzzz"), "unknown");
    }

    #[test]
    fn commit_url_requires_hex_sha() {
        assert_eq!(commit_url("unknown"), None);
        assert_eq!(commit_url("not-a-sha"), None);
        assert_eq!(commit_url("../oops"), None);
        assert_eq!(
            commit_url("0123456789abcdef0123456789abcdef01234567"),
            Some(format!(
                "{REPO_URL}/commit/0123456789abcdef0123456789abcdef01234567"
            ))
        );
        assert!(valid_git_sha("0123456789abcdef0123456789abcdef01234567"));
        assert!(!valid_git_sha(""));
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
        assert_eq!(release_url(Some("../evil")), format!("{REPO_URL}/releases"));
        assert_eq!(release_label(Some("../evil"), VERSION), VERSION);
        assert_eq!(
            release_url(Some("v1 with space")),
            format!("{REPO_URL}/releases")
        );
        assert!(valid_git_tag("release/v0.1.0"));
        assert!(!valid_git_tag(".."));
        assert!(!valid_git_tag("/abs"));
        assert_eq!(encode_tag("v0.1.0"), "v0.1.0");
        assert_eq!(encode_tag("rel/v1"), "rel/v1");
    }
}
