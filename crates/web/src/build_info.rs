use std::path::Path;

pub const REPO_URL: &str = "https://github.com/ataias/flashcards";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

const PACK_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/pack");

fn pack_dir() -> &'static Path {
    Path::new(PACK_DIR)
}

fn read_pack_file(name: &str) -> Option<String> {
    read_pack_file_from(pack_dir(), name)
}

fn read_pack_file_from(dir: &Path, name: &str) -> Option<String> {
    if name.is_empty() || name.bytes().any(|b| matches!(b, b'/' | b'\\' | b'\0')) {
        return None;
    }
    let raw = std::fs::read_to_string(dir.join(name)).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn compile_time_sha() -> &'static str {
    let sha = env!("FLASHCARDS_GIT_SHA");
    if valid_git_sha(sha) { sha } else { "unknown" }
}

/// Prefer the pack-time file in the image; fall back to compile-time bake, then `unknown`.
pub fn git_sha() -> String {
    if let Some(sha) = read_pack_file("git-sha").filter(|sha| valid_git_sha(sha)) {
        return sha;
    }
    compile_time_sha().to_string()
}

/// Prefer the pack-time file in the image; fall back to optional compile-time tag.
pub fn git_tag() -> Option<String> {
    if let Some(tag) = read_pack_file("git-tag").filter(|tag| valid_git_tag(tag)) {
        return Some(tag);
    }
    option_env!("FLASHCARDS_GIT_TAG")
        .filter(|tag| valid_git_tag(tag))
        .map(str::to_string)
}

/// Uncompressed per-arch image size written at pack time. Missing locally / in CI.
pub fn uncompressed_image_size_bytes() -> Option<u64> {
    parse_image_size_bytes(read_pack_file("image-size-bytes")?.as_str())
}

pub fn image_size_label(bytes: Option<u64>) -> String {
    match bytes {
        Some(n) => format_byte_size(n),
        None => "unknown".into(),
    }
}

fn parse_image_size_bytes(raw: &str) -> Option<u64> {
    let s = raw.trim();
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok().filter(|&n| n > 0)
}

fn format_byte_size(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * 1024;
    if bytes >= MIB {
        let tenths = bytes.saturating_mul(10).saturating_add(MIB / 2) / MIB;
        format!("{}.{} MiB", tenths / 10, tenths % 10)
    } else if bytes >= KIB {
        let tenths = bytes.saturating_mul(10).saturating_add(KIB / 2) / KIB;
        format!("{}.{} KiB", tenths / 10, tenths % 10)
    } else {
        format!("{bytes} B")
    }
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
        REPO_URL, VERSION, commit_url, encode_tag, format_byte_size, git_sha, image_size_label,
        parse_image_size_bytes, read_pack_file_from, release_label, release_url, short_sha,
        valid_git_sha, valid_git_tag,
    };

    #[test]
    fn baked_or_pack_sha_is_unknown_or_hex() {
        let sha = git_sha();
        assert!(sha == "unknown" || valid_git_sha(&sha), "{sha}");
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

    #[test]
    fn pack_files_trim_and_reject_junk_names() {
        let dir = std::env::temp_dir().join(format!(
            "flashcards-pack-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("git-sha"), "  0123456789abcdef  \n").unwrap();
        std::fs::write(dir.join("git-tag"), "v0.1.0\n").unwrap();
        std::fs::write(dir.join("image-size-bytes"), "12345678\n").unwrap();
        std::fs::write(dir.join("empty"), "   \n").unwrap();

        assert_eq!(
            read_pack_file_from(&dir, "git-sha").as_deref(),
            Some("0123456789abcdef")
        );
        assert_eq!(
            read_pack_file_from(&dir, "git-tag").as_deref(),
            Some("v0.1.0")
        );
        assert_eq!(
            read_pack_file_from(&dir, "image-size-bytes").as_deref(),
            Some("12345678")
        );
        assert_eq!(read_pack_file_from(&dir, "empty"), None);
        assert_eq!(read_pack_file_from(&dir, "missing"), None);
        assert_eq!(read_pack_file_from(&dir, "../git-sha"), None);
        assert_eq!(read_pack_file_from(&dir, "a/b"), None);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn image_size_parses_bytes_and_labels_unknown() {
        assert_eq!(parse_image_size_bytes("12345678"), Some(12_345_678));
        assert_eq!(parse_image_size_bytes("  99\n"), Some(99));
        assert_eq!(parse_image_size_bytes("0"), None);
        assert_eq!(parse_image_size_bytes(""), None);
        assert_eq!(parse_image_size_bytes("-1"), None);
        assert_eq!(parse_image_size_bytes("12.3"), None);
        assert_eq!(parse_image_size_bytes("12 MiB"), None);
        assert_eq!(image_size_label(None), "unknown");
        assert_eq!(image_size_label(Some(512)), "512 B");
        assert_eq!(image_size_label(Some(1024)), "1.0 KiB");
        assert_eq!(format_byte_size(5 * 1024 * 1024), "5.0 MiB");
    }
}
