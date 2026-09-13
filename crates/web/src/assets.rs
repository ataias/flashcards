use std::collections::HashMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::SystemTime;

use sha2::{Digest, Sha256};

use crate::error::AppError;
use crate::static_dir;

struct CachedHash {
    mtime: SystemTime,
    hex: String,
}

static HASHES: LazyLock<Mutex<HashMap<PathBuf, CachedHash>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub(crate) struct Head {
    pub title: String,
    pub css_href: String,
    pub htmx_src: String,
    pub perf_footer_src: String,
}

impl Head {
    pub(crate) fn new(title: impl Into<String>) -> Result<Self, AppError> {
        Ok(Self {
            title: title.into(),
            css_href: hashed_href("app.css")?,
            htmx_src: hashed_href("htmx.min.js")?,
            perf_footer_src: hashed_href("perf-footer.js")?,
        })
    }
}

fn hashed_href(filename: &str) -> Result<String, AppError> {
    hashed_href_in(Path::new(static_dir()), filename)
}

pub(crate) fn hashed_href_in(dir: &Path, filename: &str) -> Result<String, AppError> {
    let path = dir.join(filename);
    let mtime = fs::metadata(&path)
        .and_then(|meta| meta.modified())
        .map_err(AppError::static_asset)?;

    {
        let cache = HASHES.lock().unwrap_or_else(|err| err.into_inner());
        if let Some(cached) = cache.get(&path)
            && cached.mtime == mtime
        {
            return Ok(static_url(filename, &cached.hex));
        }
    }

    let bytes = fs::read(&path).map_err(AppError::static_asset)?;
    let hex = sha256_prefix16(&bytes);
    {
        let mut cache = HASHES.lock().unwrap_or_else(|err| err.into_inner());
        cache.insert(
            path,
            CachedHash {
                mtime,
                hex: hex.clone(),
            },
        );
    }
    Ok(static_url(filename, &hex))
}

fn static_url(filename: &str, hex: &str) -> String {
    format!("/static/{filename}?h={hex}")
}

fn sha256_prefix16(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest
        .iter()
        .take(8)
        .fold(String::with_capacity(16), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn temp_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "flashcards-assets-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn hash_is_16_lowercase_hex_of_sha256() {
        let dir = temp_dir();
        let file = dir.join("app.css");
        fs::write(&file, b"body{}\n").unwrap();
        let href = hashed_href_in(&dir, "app.css").unwrap();
        let expected = sha256_prefix16(b"body{}\n");
        assert_eq!(href, format!("/static/app.css?h={expected}"));
        assert_eq!(expected.len(), 16);
        assert!(
            expected
                .bytes()
                .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_is_error() {
        let dir = temp_dir();
        let err = hashed_href_in(&dir, "missing.css").unwrap_err();
        assert!(matches!(err, AppError::StaticAsset(_)));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn recomputes_when_mtime_changes() {
        let dir = temp_dir();
        let file = dir.join("app.css");
        fs::write(&file, b"v1").unwrap();
        let first = hashed_href_in(&dir, "app.css").unwrap();
        fs::write(&file, b"v2").unwrap();
        let handle = fs::File::open(&file).unwrap();
        let later = SystemTime::now() + Duration::from_secs(2);
        handle.set_modified(later).unwrap();
        drop(handle);
        let second = hashed_href_in(&dir, "app.css").unwrap();
        assert_ne!(first, second);
        assert_eq!(
            second,
            format!("/static/app.css?h={}", sha256_prefix16(b"v2"))
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
